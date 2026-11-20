//! The agentiscale network's agents in the sidebar's agent panel.
//!
//! The local agentiscaled node lists every agent in the owner's network, on
//! any machine, with what it is doing (`GET /network/agents` on its
//! `agent.sock`; see `crate::client::network_watch`). The panel shows them
//! below this machine's pane agents, under a "network" heading.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use serde::Deserialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use super::*;

/// What the local node last said about the network.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NetworkView {
    Agents(NetworkAgents),
    /// The node answered but could not list the agents; the text says why.
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct NetworkAgents {
    /// This node's peer ID, which tells this machine's agents apart.
    #[serde(default)]
    pub(crate) peer_id: String,
    #[serde(default)]
    pub(crate) agents: Vec<NetworkAgent>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct NetworkAgent {
    pub(crate) peer_id: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) reported_at: Option<String>,
    /// `fresh` while the agent's machine reports to the network.
    #[serde(default)]
    pub(crate) report_status: String,
    #[serde(default)]
    pub(crate) activity: Option<NetworkActivity>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub(crate) struct NetworkActivity {
    #[serde(default)]
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) since: Option<String>,
    #[serde(default)]
    pub(crate) waiting: u32,
    #[serde(default)]
    pub(crate) waiting_since: Option<String>,
}

impl ClientShellState {
    /// Keeps what the node said; true when the sidebar must be drawn again.
    pub(crate) fn set_network(&mut self, view: Option<NetworkView>) -> bool {
        if self.network == view {
            return false;
        }
        self.network = view;
        true
    }
}

/// One line of the network group.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum NetworkLine {
    Heading,
    Message(String),
    Agent {
        name: String,
        status: crate::api::schema::AgentStatus,
        state: String,
        machine: String,
    },
}

/// The lines the network group shows, or none when no node answers.
pub(super) fn network_lines(view: Option<&NetworkView>, now: OffsetDateTime) -> Vec<NetworkLine> {
    let Some(view) = view else {
        return Vec::new();
    };
    let mut lines = vec![NetworkLine::Heading];
    match view {
        NetworkView::Unavailable(message) => lines.push(NetworkLine::Message(message.clone())),
        NetworkView::Agents(list) if list.agents.is_empty() => {
            lines.push(NetworkLine::Message("no agents offered yet".to_owned()))
        }
        NetworkView::Agents(list) => {
            let mut agents = list.agents.iter().collect::<Vec<_>>();
            // Available agents first, then by name.
            agents.sort_by(|a, b| {
                (a.report_status != "fresh", &a.name, &a.peer_id).cmp(&(
                    b.report_status != "fresh",
                    &b.name,
                    &b.peer_id,
                ))
            });
            for agent in agents {
                let (status, state) = agent_state(agent, now);
                let machine = if agent.peer_id == list.peer_id {
                    "this machine".to_owned()
                } else {
                    short_peer_id(&agent.peer_id)
                };
                lines.push(NetworkLine::Agent {
                    name: agent.name.clone(),
                    status,
                    state,
                    machine,
                });
            }
        }
    }
    lines
}

/// The agent's status icon and its state in words: busy, waiting for an
/// answer, idle or offline, with how long.
fn agent_state(
    agent: &NetworkAgent,
    now: OffsetDateTime,
) -> (crate::api::schema::AgentStatus, String) {
    use crate::api::schema::AgentStatus;
    if agent.report_status != "fresh" {
        let seen = since(agent.reported_at.as_deref(), now)
            .map(|ago| format!("offline {ago}"))
            .unwrap_or_else(|| "offline".to_owned());
        return (AgentStatus::Unknown, seen);
    }
    let Some(activity) = agent.activity.as_ref() else {
        return (AgentStatus::Idle, "idle".to_owned());
    };
    let waiting = match activity.waiting {
        0 => None,
        1 => Some("waiting for an answer".to_owned()),
        n => Some(format!("waiting for {n} answers")),
    }
    .map(|text| with_since(text, activity.waiting_since.as_deref(), now));
    if activity.state == "busy" {
        let busy = with_since("busy".to_owned(), activity.since.as_deref(), now);
        return match waiting {
            Some(waiting) => (AgentStatus::Working, format!("{busy}, {waiting}")),
            None => (AgentStatus::Working, busy),
        };
    }
    match waiting {
        Some(waiting) => (AgentStatus::Blocked, waiting),
        None => (AgentStatus::Idle, "idle".to_owned()),
    }
}

fn with_since(text: String, at: Option<&str>, now: OffsetDateTime) -> String {
    match since(at, now) {
        Some(ago) => format!("{text} {ago}"),
        None => text,
    }
}

/// How long ago `at` was, rounded for a glance: "45s", "12m", "2h5m".
fn since(at: Option<&str>, now: OffsetDateTime) -> Option<String> {
    let at = OffsetDateTime::parse(at?, &Rfc3339).ok()?;
    let seconds = (now - at).whole_seconds().max(0);
    Some(match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s => format!("{}h{}m", s / 3600, (s % 3600) / 60),
    })
}

/// The end of a peer ID, which tells machines apart at a glance.
fn short_peer_id(id: &str) -> String {
    let chars = id.chars().collect::<Vec<_>>();
    if chars.len() <= 12 {
        return id.to_owned();
    }
    format!("…{}", chars[chars.len() - 6..].iter().collect::<String>())
}

impl NetworkLine {
    /// Rows the line takes: an agent shows its name and machine, then its
    /// state, as the pane agents above it do.
    pub(super) fn height(&self) -> usize {
        match self {
            NetworkLine::Agent { .. } => 2,
            NetworkLine::Heading | NetworkLine::Message(_) => 1,
        }
    }
}

pub(super) fn render_network_line(
    buffer: &mut Buffer,
    rect: Rect,
    line: &NetworkLine,
    config: &ClientShellConfig,
) {
    let palette = &config.palette;
    let secondary = Style::default().fg(palette.overlay0);
    let lines = match line {
        NetworkLine::Heading => vec![Line::from(Span::styled(
            " network",
            secondary.add_modifier(Modifier::BOLD),
        ))],
        NetworkLine::Message(message) => vec![Line::from(Span::styled(
            format!("   {message}"),
            secondary.add_modifier(Modifier::DIM),
        ))],
        NetworkLine::Agent {
            name,
            status,
            state,
            machine,
        } => {
            let color = status_color(*status, palette);
            let offline = *status == crate::api::schema::AgentStatus::Unknown;
            let name_style = if offline {
                secondary.add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(palette.subtext0)
                    .add_modifier(Modifier::BOLD)
            };
            vec![
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        status_icon(*status, config.status_indicators),
                        Style::default().fg(color),
                    ),
                    Span::raw(" "),
                    Span::styled(name.clone(), name_style),
                    Span::styled(format!(" · {machine}"), secondary),
                ]),
                Line::from(vec![
                    Span::raw("   "),
                    Span::styled(state.clone(), Style::default().fg(color)),
                ]),
            ]
        }
    };
    for (index, line) in lines.into_iter().take(rect.height as usize).enumerate() {
        Paragraph::new(line).render(
            Rect::new(rect.x, rect.y + index as u16, rect.width, 1),
            buffer,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::AgentStatus;

    fn at(seconds_ago: i64, now: OffsetDateTime) -> Option<String> {
        Some(
            (now - time::Duration::seconds(seconds_ago))
                .format(&Rfc3339)
                .unwrap(),
        )
    }

    fn agent(
        peer: &str,
        name: &str,
        fresh: bool,
        activity: Option<NetworkActivity>,
    ) -> NetworkAgent {
        NetworkAgent {
            peer_id: peer.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            reported_at: None,
            report_status: if fresh { "fresh" } else { "stale" }.to_owned(),
            activity,
        }
    }

    #[test]
    fn no_node_shows_nothing_and_a_refusal_shows_why() {
        let now = OffsetDateTime::now_utc();
        assert!(network_lines(None, now).is_empty());
        assert_eq!(
            network_lines(
                Some(&NetworkView::Unavailable("node not running".to_owned())),
                now
            ),
            vec![
                NetworkLine::Heading,
                NetworkLine::Message("node not running".to_owned())
            ]
        );
    }

    #[test]
    fn agents_show_state_and_machine_with_available_first() {
        let now = OffsetDateTime::now_utc();
        let mut gone = agent("12D3KooWOtherMachineAAAAAA", "archive", false, None);
        gone.reported_at = at(12 * 60, now);
        let list = NetworkAgents {
            peer_id: "12D3KooWSelfSelfSelfSelf".to_owned(),
            agents: vec![
                gone,
                agent(
                    "12D3KooWSelfSelfSelfSelf",
                    "codex",
                    true,
                    Some(NetworkActivity {
                        state: "busy".to_owned(),
                        since: at(3 * 60 + 5, now),
                        waiting: 0,
                        waiting_since: None,
                    }),
                ),
                agent(
                    "12D3KooWOtherMachineBBBBBB",
                    "reviewer",
                    true,
                    Some(NetworkActivity {
                        state: "idle".to_owned(),
                        since: None,
                        waiting: 1,
                        waiting_since: at(42, now),
                    }),
                ),
                agent("12D3KooWOtherMachineBBBBBB", "writer", true, None),
            ],
        };
        let lines = network_lines(Some(&NetworkView::Agents(list)), now);
        let expected = |name: &str, status, state: &str, machine: &str| NetworkLine::Agent {
            name: name.to_owned(),
            status,
            state: state.to_owned(),
            machine: machine.to_owned(),
        };
        assert_eq!(
            lines,
            vec![
                NetworkLine::Heading,
                expected("codex", AgentStatus::Working, "busy 3m", "this machine"),
                expected(
                    "reviewer",
                    AgentStatus::Blocked,
                    "waiting for an answer 42s",
                    "…BBBBBB"
                ),
                expected("writer", AgentStatus::Idle, "idle", "…BBBBBB"),
                expected("archive", AgentStatus::Unknown, "offline 12m", "…AAAAAA"),
            ]
        );
    }

    #[test]
    fn an_empty_network_says_so() {
        let now = OffsetDateTime::now_utc();
        let lines = network_lines(
            Some(&NetworkView::Agents(NetworkAgents {
                peer_id: String::new(),
                agents: Vec::new(),
            })),
            now,
        );
        assert_eq!(
            lines[1],
            NetworkLine::Message("no agents offered yet".to_owned())
        );
    }

    #[test]
    fn the_node_answer_parses() {
        let body = r#"{"network_id":"n","peer_id":"p","observed_at":"2026-11-20T13:00:00Z","stale_after_seconds":300,
            "agents":[{"peer_id":"p","name":"codex","description":"d","reported_at":"2026-11-20T12:59:30Z",
            "report_status":"fresh","activity":{"state":"busy","since":"2026-11-20T12:57:00.123456789Z","waiting":0}}]}"#;
        let list: NetworkAgents = serde_json::from_str(body).unwrap();
        assert_eq!(list.agents[0].activity.as_ref().unwrap().state, "busy");
        let now = OffsetDateTime::parse("2026-11-20T13:00:00Z", &Rfc3339).unwrap();
        let (_, state) = agent_state(&list.agents[0], now);
        assert_eq!(state, "busy 2m");
    }
}
