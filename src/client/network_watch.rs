//! Polls the local agentiscaled node for the agents of the owner's
//! agentiscale network, for the sidebar's network group.
//!
//! The node answers `GET /network/agents` on the `agent.sock` socket in its
//! data directory. It reads the list from the control service with its own
//! key, so agentiscaler holds no account credential. The data directory is
//! agentiscaled's default (the user configuration directory's
//! `agentiscale`), or `AGENTISCALER_NODE_DATA_DIR`.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::shell::{NetworkAgents, NetworkView};
use super::ClientLoopEvent;

/// How often the node is asked. The node itself asks the control service
/// at most once per five seconds, however many tools ask it.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// The node may ask the control service before it answers.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_ANSWER_BYTES: u64 = 4 << 20;

pub(super) fn watch(
    event_tx: tokio::sync::mpsc::Sender<ClientLoopEvent>,
    should_quit: Arc<AtomicBool>,
) {
    let Some(data_dir) = node_data_dir() else {
        return;
    };
    std::thread::spawn(move || {
        let mut previous: Option<Option<NetworkView>> = None;
        while !should_quit.load(Ordering::Acquire) {
            let current = read_view(&data_dir);
            if previous.as_ref() != Some(&current) {
                previous = Some(current.clone());
                if event_tx
                    .blocking_send(ClientLoopEvent::NetworkAgents(current))
                    .is_err()
                {
                    break;
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

/// agentiscaled's data directory: `AGENTISCALER_NODE_DATA_DIR`, or the
/// default agentiscaled uses (Go's os.UserConfigDir joined with
/// `agentiscale`).
fn node_data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("AGENTISCALER_NODE_DATA_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
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

/// What the node says now: None when no node is set up here, so the
/// sidebar shows no network group at all.
fn read_view(data_dir: &Path) -> Option<NetworkView> {
    let joined = data_dir.join("agentiscaled.json").exists();
    match fetch(&data_dir.join("agent.sock")) {
        Err(_) if !joined => None,
        Err(_) => Some(NetworkView::Unavailable(
            "agentiscaled is not running".to_owned(),
        )),
        Ok((status, body)) => Some(view_of(status, &body)),
    }
}

fn view_of(status: u16, body: &[u8]) -> NetworkView {
    if status == 200 {
        return match serde_json::from_slice::<NetworkAgents>(body) {
            Ok(list) => NetworkView::Agents(list),
            Err(_) => NetworkView::Unavailable("agentiscaled sent an unreadable list".to_owned()),
        };
    }
    #[derive(serde::Deserialize)]
    struct Failure {
        error: String,
    }
    match serde_json::from_slice::<Failure>(body) {
        Ok(failure) if !failure.error.is_empty() => NetworkView::Unavailable(failure.error),
        // A node from before network listing has no such route.
        _ => NetworkView::Unavailable("upgrade agentiscaled to list network agents".to_owned()),
    }
}

/// One HTTP/1.0 request on the node's socket; the node closes the
/// connection after its answer.
#[cfg(unix)]
fn fetch(socket: &Path) -> io::Result<(u16, Vec<u8>)> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(ANSWER_TIMEOUT))?;
    stream.set_write_timeout(Some(ANSWER_TIMEOUT))?;
    stream.write_all(b"GET /network/agents HTTP/1.0\r\nHost: agentiscaled\r\n\r\n")?;
    let mut answer = Vec::new();
    stream.take(MAX_ANSWER_BYTES).read_to_end(&mut answer)?;
    parse_response(&answer)
}

#[cfg(not(unix))]
fn fetch(_socket: &Path) -> io::Result<(u16, Vec<u8>)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the agentiscaled socket is not supported on this platform yet",
    ))
}

fn parse_response(answer: &[u8]) -> io::Result<(u16, Vec<u8>)> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid HTTP answer");
    let split = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(invalid)?;
    let head = std::str::from_utf8(&answer[..split]).map_err(|_| invalid())?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(invalid)?;
    Ok((status, answer[split + 4..].to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_map_to_views() {
        let body =
            br#"{"peer_id":"p","agents":[{"peer_id":"p","name":"codex","report_status":"fresh"}]}"#;
        match view_of(200, body) {
            NetworkView::Agents(list) => assert_eq!(list.agents[0].name, "codex"),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            view_of(
                410,
                br#"{"error":"this node was removed from the network"}"#
            ),
            NetworkView::Unavailable("this node was removed from the network".to_owned())
        );
        assert_eq!(
            view_of(404, b"404 page not found\n"),
            NetworkView::Unavailable("upgrade agentiscaled to list network agents".to_owned())
        );
        assert!(matches!(view_of(200, b"nope"), NetworkView::Unavailable(_)));
    }

    #[test]
    fn http_answer_parses() {
        let (status, body) =
            parse_response(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{}").unwrap();
        assert_eq!((status, body.as_slice()), (200, b"{}".as_slice()));
        assert!(parse_response(b"garbage").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn no_node_hides_the_group_and_a_stopped_node_says_so() {
        let dir = std::env::temp_dir().join(format!("agentiscaler-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_view(&dir), None);
        std::fs::write(dir.join("agentiscaled.json"), "{}").unwrap();
        assert_eq!(
            read_view(&dir),
            Some(NetworkView::Unavailable(
                "agentiscaled is not running".to_owned()
            ))
        );

        let socket = dir.join("agent.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut request = [0u8; 256];
            let n = conn.read(&mut request).unwrap();
            assert!(std::str::from_utf8(&request[..n])
                .unwrap()
                .starts_with("GET /network/agents HTTP/1.0\r\n"));
            conn.write_all(b"HTTP/1.1 200 OK\r\n\r\n{\"peer_id\":\"p\",\"agents\":[]}")
                .unwrap();
        });
        let view = read_view(&dir);
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            view,
            Some(NetworkView::Agents(NetworkAgents {
                peer_id: "p".to_owned(),
                agents: Vec::new()
            }))
        );
    }
}
