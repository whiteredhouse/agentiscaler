//! agentiscaler: names this fork shows where upstream herdr shows its own.
//!
//! Kept in one file so upstream merges touch as few upstream lines as
//! possible.

/// Rewrites the bundled herdr agent skill for the agentiscaler binary. In the
/// skill the lowercase word `herdr` always names the command (`herdr agent
/// list`, the `herdr` binary in `PATH`); `Herdr` in prose and `HERDR_*`
/// variables stay as they are, since panes still use those variables.
pub(crate) fn agent_skill(upstream: &str) -> String {
    let word = regex::Regex::new(r"\bherdr\b").expect("valid regex");
    word.replace_all(upstream, "agentiscaler").into_owned()
}

#[cfg(test)]
mod tests {
    use super::agent_skill;

    #[test]
    fn agent_skill_names_the_agentiscaler_command() {
        let skill = agent_skill("name: herdr\nRun `herdr agent list`. Herdr sets HERDR_PANE_ID.\n");
        assert_eq!(
            skill,
            "name: agentiscaler\nRun `agentiscaler agent list`. Herdr sets HERDR_PANE_ID.\n"
        );
    }

    #[test]
    fn bundled_agent_skill_has_no_herdr_command_left() {
        let skill = agent_skill(include_str!("../skills/herdr/SKILL.md"));
        assert!(!regex::Regex::new(r"\bherdr\b").unwrap().is_match(&skill));
        assert!(skill.contains("agentiscaler agent"));
    }
}
