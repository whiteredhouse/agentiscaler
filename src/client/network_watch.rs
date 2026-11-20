//! Watches the local agentiscaled node for the agents of the owner's
//! agentiscale network, for the sidebar's network group.
//!
//! The node streams `GET /network/agents?watch=1` on the `agent.sock`
//! socket in its data directory: the list at once, then again on each
//! change, one JSON object per line. It reads the list from the control
//! service with its own key, and only while someone watches, so
//! agentiscaler holds no account credential. The data directory is
//! `[agentiscale] node_data_dir`, or agentiscaled's default (the user
//! configuration directory's `agentiscale`).

use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::shell::{NetworkAgents, NetworkView};
use super::ClientLoopEvent;

/// How long to wait before watching again after the node closed the stream
/// or could not be reached.
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// A line of the stream is one list; a longer one is not the node's.
const MAX_LINE_BYTES: u64 = 4 << 20;

pub(super) fn watch(
    event_tx: tokio::sync::mpsc::Sender<ClientLoopEvent>,
    should_quit: Arc<AtomicBool>,
    configured_dir: &str,
) {
    // agentiscaled's local socket is a Unix socket; elsewhere the network
    // group stays hidden.
    if cfg!(not(unix)) {
        return;
    }
    let Some(data_dir) = node_data_dir(configured_dir) else {
        return;
    };
    std::thread::spawn(move || {
        let mut previous: Option<Option<NetworkView>> = None;
        let mut publish = |view: Option<NetworkView>| -> bool {
            if previous.as_ref() == Some(&view) {
                return true;
            }
            previous = Some(view.clone());
            event_tx
                .blocking_send(ClientLoopEvent::NetworkAgents(view))
                .is_ok()
        };
        while !should_quit.load(Ordering::Acquire) {
            let ended = follow(&data_dir, &should_quit, &mut publish);
            if !ended {
                break;
            }
            std::thread::sleep(RETRY_INTERVAL);
        }
    });
}

/// agentiscaled's data directory: the configured one, or the default
/// agentiscaled uses (Go's os.UserConfigDir joined with `agentiscale`).
fn node_data_dir(configured: &str) -> Option<PathBuf> {
    if !configured.trim().is_empty() {
        return Some(PathBuf::from(configured.trim()));
    }
    user_config_dir().map(|dir| dir.join("agentiscale"))
}

fn user_config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library").join("Application Support"));
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return Some(dir);
        }
    }
    home.map(|home| home.join(".config"))
}

/// Follows one watch until the node ends it, publishing each view. It
/// returns false when the client is gone and watching should stop.
fn follow(
    data_dir: &Path,
    should_quit: &AtomicBool,
    publish: &mut impl FnMut(Option<NetworkView>) -> bool,
) -> bool {
    let joined = data_dir.join("agentiscaled.json").exists();
    let unreachable = || {
        // No node set up here: no network group at all.
        joined.then(|| NetworkView::Unavailable("agentiscaled is not running".to_owned()))
    };
    let mut reader = match open_watch(&data_dir.join("agent.sock")) {
        Ok(reader) => reader,
        Err(_) => return publish(unreachable()),
    };
    let status = match read_status(&mut reader) {
        Ok(status) => status,
        Err(_) => return publish(unreachable()),
    };
    if status != 200 {
        let mut body = Vec::new();
        let _ = reader.take(MAX_LINE_BYTES).read_to_end(&mut body);
        return publish(Some(failure_view(&body)));
    }
    loop {
        if should_quit.load(Ordering::Acquire) {
            return false;
        }
        let mut line = Vec::new();
        match Read::by_ref(&mut reader)
            .take(MAX_LINE_BYTES)
            .read_until(b'\n', &mut line)
        {
            Ok(0) | Err(_) => return publish(unreachable()),
            Ok(_) => {}
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if !publish(Some(line_view(&line))) {
            return false;
        }
    }
}

/// One line of the stream: the list, or why the node can't read it.
fn line_view(line: &[u8]) -> NetworkView {
    if let Ok(list) = serde_json::from_slice::<NetworkAgents>(line) {
        if !is_failure(line) {
            return NetworkView::Agents(list);
        }
    }
    failure_view(line)
}

#[derive(serde::Deserialize)]
struct Failure {
    error: String,
}

fn is_failure(body: &[u8]) -> bool {
    serde_json::from_slice::<Failure>(body).is_ok_and(|failure| !failure.error.is_empty())
}

fn failure_view(body: &[u8]) -> NetworkView {
    match serde_json::from_slice::<Failure>(body) {
        Ok(failure) if !failure.error.is_empty() => NetworkView::Unavailable(failure.error),
        // A node from before network listing has no such route.
        _ => NetworkView::Unavailable("upgrade agentiscaled to list network agents".to_owned()),
    }
}

/// Starts an HTTP/1.0 watch on the node's socket; the node streams the
/// body until it closes the connection.
#[cfg(unix)]
fn open_watch(socket: &Path) -> io::Result<BufReader<std::os::unix::net::UnixStream>> {
    use std::io::Write;
    let mut stream = std::os::unix::net::UnixStream::connect(socket)?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET /network/agents?watch=1 HTTP/1.0\r\nHost: agentiscaled\r\n\r\n")?;
    Ok(BufReader::new(stream))
}

#[cfg(not(unix))]
fn open_watch(_socket: &Path) -> io::Result<BufReader<io::Empty>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the agentiscaled socket is not supported on this platform yet",
    ))
}

/// Reads the status line and headers, leaving the reader at the body.
fn read_status(reader: &mut impl BufRead) -> io::Result<u16> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP answer");
    let mut status_line = String::new();
    Read::by_ref(reader)
        .take(8 << 10)
        .read_line(&mut status_line)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(invalid)?;
    loop {
        let mut header = String::new();
        if Read::by_ref(reader).take(8 << 10).read_line(&mut header)? == 0 {
            return Err(invalid());
        }
        if header == "\r\n" || header == "\n" {
            return Ok(status);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_map_to_views() {
        let list = br#"{"peer_id":"p","agents":[{"peer_id":"p","name":"codex","machine_name":"mbp","report_status":"fresh"}]}"#;
        match line_view(list) {
            NetworkView::Agents(list) => {
                assert_eq!(list.agents[0].name, "codex");
                assert_eq!(list.agents[0].machine_name, "mbp");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            line_view(br#"{"error":"this node was removed from the network"}"#),
            NetworkView::Unavailable("this node was removed from the network".to_owned())
        );
        assert_eq!(
            failure_view(b"404 page not found\n"),
            NetworkView::Unavailable("upgrade agentiscaled to list network agents".to_owned())
        );
    }

    #[test]
    fn status_and_headers_are_read_up_to_the_body() {
        let mut reader =
            io::Cursor::new(b"HTTP/1.1 200 OK\r\nContent-Type: x\r\n\r\n{}\n".to_vec());
        assert_eq!(read_status(&mut reader).unwrap(), 200);
        let mut rest = String::new();
        reader.read_to_string(&mut rest).unwrap();
        assert_eq!(rest, "{}\n");
        assert!(read_status(&mut io::Cursor::new(b"garbage".to_vec())).is_err());
    }

    #[test]
    fn configured_directory_wins() {
        assert_eq!(
            node_data_dir(" /srv/node "),
            Some(PathBuf::from("/srv/node"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_watch_publishes_each_change_and_the_end() {
        use std::io::Write;
        use std::os::unix::net::UnixListener;
        let dir = std::env::temp_dir().join(format!("agentiscaler-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let quit = AtomicBool::new(false);
        let mut seen = Vec::new();
        // No node set up: the group stays hidden.
        assert!(follow(&dir, &quit, &mut |view| {
            seen.push(view);
            true
        }));
        assert_eq!(seen, vec![None]);

        std::fs::write(dir.join("agentiscaled.json"), "{}").unwrap();
        let listener = UnixListener::bind(dir.join("agent.sock")).unwrap();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut request = [0u8; 256];
            let n = conn.read(&mut request).unwrap();
            assert!(std::str::from_utf8(&request[..n])
                .unwrap()
                .starts_with("GET /network/agents?watch=1 HTTP/1.0\r\n"));
            conn.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\n\r\n")
                .unwrap();
            conn.write_all(b"{\"peer_id\":\"p\",\"agents\":[]}\n")
                .unwrap();
            conn.write_all(b"{\"error\":\"the control service could not be reached\"}\n")
                .unwrap();
        });
        let mut seen = Vec::new();
        assert!(follow(&dir, &quit, &mut |view| {
            seen.push(view);
            true
        }));
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            seen,
            vec![
                Some(NetworkView::Agents(NetworkAgents {
                    peer_id: "p".to_owned(),
                    agents: Vec::new()
                })),
                Some(NetworkView::Unavailable(
                    "the control service could not be reached".to_owned()
                )),
                Some(NetworkView::Unavailable(
                    "agentiscaled is not running".to_owned()
                )),
            ]
        );
    }
}
