use super::AgentPanelEntry;
use crate::config::{
    AgentSidebarToken, AgentsSidebarConfig, SidebarTokenStyle, SpaceSidebarToken,
    SpacesSidebarConfig,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedToken {
    pub kind: ResolvedTokenKind,
    pub style: SidebarTokenStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedTokenKind {
    StateIcon,
    StateText(String),
    Workspace(String),
    Tab(String),
    Pane(String),
    Agent(String),
    TerminalTitle(String),
    Branch(String),
    GitStatus {
        ahead: usize,
        behind: usize,
    },
    /// The rendered agent summary for a space, e.g. "3 agents · 1 need you".
    SpaceAgents(String),
    NeedEdge {
        state: crate::detect::AgentState,
        seen: bool,
    },
    Waiting {
        text: String,
        state: crate::detect::AgentState,
    },
    Custom(String),
}

/// True when the agent is sitting on Felipe's time: finished and not yet
/// looked at, or blocked on a question.
pub(super) fn needs_attention(state: crate::detect::AgentState, seen: bool) -> bool {
    matches!(
        (state, seen),
        (crate::detect::AgentState::Blocked, _) | (crate::detect::AgentState::Idle, false)
    )
}

fn waiting_text(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("waiting {secs}s")
    } else if secs < 3600 {
        format!("waiting {}m", secs / 60)
    } else {
        format!("waiting {}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

impl ResolvedToken {
    fn new(kind: ResolvedTokenKind, style: SidebarTokenStyle) -> Self {
        Self { kind, style }
    }

    #[cfg(test)]
    pub(super) fn unstyled(kind: ResolvedTokenKind) -> Self {
        Self::new(kind, SidebarTokenStyle::default())
    }
}

pub(super) fn agent_rows(
    config: &AgentsSidebarConfig,
    entry: &AgentPanelEntry,
    state_text: &str,
) -> Vec<Vec<ResolvedToken>> {
    config
        .rows_for_agent(entry.agent)
        .iter()
        .filter_map(|row| {
            let resolved = row
                .iter()
                .filter_map(|configured| {
                    let (token, style) = configured.parts();
                    let kind = match token {
                        AgentSidebarToken::StateIcon => Some(ResolvedTokenKind::StateIcon),
                        AgentSidebarToken::StateText => {
                            Some(ResolvedTokenKind::StateText(state_text.to_string()))
                        }
                        AgentSidebarToken::Workspace => {
                            Some(ResolvedTokenKind::Workspace(entry.primary_label.clone()))
                        }
                        AgentSidebarToken::Tab => {
                            entry.primary_tab_label.clone().map(ResolvedTokenKind::Tab)
                        }
                        AgentSidebarToken::Pane => {
                            entry.pane_label.clone().map(ResolvedTokenKind::Pane)
                        }
                        AgentSidebarToken::Agent => {
                            entry.agent_label.clone().map(ResolvedTokenKind::Agent)
                        }
                        AgentSidebarToken::TerminalTitle => entry
                            .terminal_title
                            .clone()
                            .map(ResolvedTokenKind::TerminalTitle),
                        AgentSidebarToken::TerminalTitleStripped => entry
                            .terminal_title_stripped
                            .clone()
                            .map(ResolvedTokenKind::TerminalTitle),
                        AgentSidebarToken::NeedEdge => Some(ResolvedTokenKind::NeedEdge {
                            state: entry.state,
                            seen: entry.seen,
                        }),
                        AgentSidebarToken::Waiting => needs_attention(entry.state, entry.seen)
                            .then_some(entry.last_agent_state_change_at)
                            .flatten()
                            .map(|at| ResolvedTokenKind::Waiting {
                                text: waiting_text(at.elapsed()),
                                state: entry.state,
                            }),
                        AgentSidebarToken::Custom(name) => entry
                            .tokens
                            .get(name)
                            .cloned()
                            .map(ResolvedTokenKind::Custom),
                        AgentSidebarToken::Styled { .. } => None,
                    }?;
                    Some(ResolvedToken::new(kind, style))
                })
                .collect::<Vec<_>>();
            // The need edge is a gutter, not content: a row carrying nothing
            // else (e.g. [need_edge, waiting] while the agent works) elides.
            let has_content = resolved
                .iter()
                .any(|token| !matches!(token.kind, ResolvedTokenKind::NeedEdge { .. }));
            has_content.then_some(resolved)
        })
        .collect()
}

pub(super) struct SpaceTokenContext<'a> {
    pub workspace: &'a str,
    pub branch: Option<&'a str>,
    pub state_text: &'a str,
    pub ahead_behind: Option<(usize, usize)>,
    pub tokens: &'a std::collections::HashMap<String, String>,
    pub suppress_git_details: bool,
    /// (total known agents in this space, how many need attention). The `agents`
    /// token renders "N agents" plus "· M need you" when M > 0, and its row elides
    /// when total is 0.
    pub agents: (usize, usize),
}

/// One-line agent summary for a space: "N agents", with "· M need you" appended
/// when M of them are blocked or finished-and-unseen. `None` when the space has no
/// known agents, so the `agents` row elides.
fn agent_summary_text(total: usize, needs_you: usize) -> Option<String> {
    if total == 0 {
        return None;
    }
    let noun = if total == 1 { "agent" } else { "agents" };
    let mut summary = format!("{total} {noun}");
    if needs_you > 0 {
        summary.push_str(&format!(" · {needs_you} need you"));
    }
    Some(summary)
}

pub(super) fn space_rows(
    config: &SpacesSidebarConfig,
    context: SpaceTokenContext<'_>,
) -> Vec<Vec<ResolvedToken>> {
    config
        .rows
        .iter()
        .filter_map(|row| {
            let resolved = row
                .iter()
                .filter_map(|configured| {
                    let (token, style) = configured.parts();
                    let kind = match token {
                        SpaceSidebarToken::StateIcon => Some(ResolvedTokenKind::StateIcon),
                        SpaceSidebarToken::StateText => {
                            Some(ResolvedTokenKind::StateText(context.state_text.to_string()))
                        }
                        SpaceSidebarToken::Workspace => {
                            Some(ResolvedTokenKind::Workspace(context.workspace.to_string()))
                        }
                        SpaceSidebarToken::Branch if !context.suppress_git_details => context
                            .branch
                            .map(|branch| ResolvedTokenKind::Branch(branch.to_string())),
                        SpaceSidebarToken::Branch => None,
                        SpaceSidebarToken::GitStatus if !context.suppress_git_details => context
                            .ahead_behind
                            .filter(|(ahead, behind)| *ahead > 0 || *behind > 0)
                            .map(|(ahead, behind)| ResolvedTokenKind::GitStatus { ahead, behind }),
                        SpaceSidebarToken::GitStatus => None,
                        SpaceSidebarToken::Agents => {
                            agent_summary_text(context.agents.0, context.agents.1)
                                .map(ResolvedTokenKind::SpaceAgents)
                        }
                        SpaceSidebarToken::Custom(name) => context
                            .tokens
                            .get(name)
                            .cloned()
                            .map(ResolvedTokenKind::Custom),
                        SpaceSidebarToken::Styled { .. } => None,
                    }?;
                    Some(ResolvedToken::new(kind, style))
                })
                .collect::<Vec<_>>();
            (!resolved.is_empty()).then_some(resolved)
        })
        .collect()
}

pub(super) fn separator(previous: &ResolvedToken, current: &ResolvedToken) -> &'static str {
    if matches!(previous.kind, ResolvedTokenKind::NeedEdge { .. }) {
        // The edge is a gutter; content hugs it.
        ""
    } else if matches!(previous.kind, ResolvedTokenKind::StateIcon)
        || matches!(current.kind, ResolvedTokenKind::GitStatus { .. })
    {
        " "
    } else {
        " · "
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentSidebarToken, SpaceSidebarToken};
    use crate::detect::AgentState;

    fn entry() -> AgentPanelEntry {
        AgentPanelEntry {
            ws_idx: 0,
            tab_idx: 0,
            pane_id: crate::layout::PaneId::from_raw(1),
            primary_label: "repo".into(),
            primary_tab_label: None,
            pane_label: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_label: Some("pi".into()),
            agent_kind_label: Some("pi".into()),
            agent: Some(crate::detect::Agent::Pi),
            state: AgentState::Working,
            seen: true,
            last_agent_state_change_seq: None,
            last_agent_state_change_at: None,
            state_labels: std::collections::HashMap::new(),
            tokens: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn missing_custom_tokens_elide_rows_and_separators() {
        let entry = entry();
        let config = AgentsSidebarConfig {
            rows: vec![
                vec![
                    AgentSidebarToken::StateIcon,
                    AgentSidebarToken::Custom("missing".into()),
                ],
                vec![AgentSidebarToken::Custom("missing".into())],
                vec![AgentSidebarToken::Agent],
            ],
            ..Default::default()
        };

        let rows = agent_rows(&config, &entry, "working");

        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            vec![ResolvedToken::unstyled(ResolvedTokenKind::StateIcon)]
        );
        assert_eq!(
            rows[1],
            vec![ResolvedToken::unstyled(ResolvedTokenKind::Agent(
                "pi".into()
            ))]
        );
    }

    #[test]
    fn state_text_and_arbitrary_values_are_independent_tokens() {
        let mut entry = entry();
        entry
            .tokens
            .insert("summary".into(), "reviewing auth".into());
        let config = AgentsSidebarConfig {
            rows: vec![vec![
                AgentSidebarToken::StateText,
                AgentSidebarToken::Custom("summary".into()),
            ]],
            ..Default::default()
        };

        assert_eq!(
            agent_rows(&config, &entry, "deep in the mines"),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateText("deep in the mines".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Custom("reviewing auth".into())),
            ]]
        );
    }

    #[test]
    fn terminal_title_builtins_are_distinct_from_custom_tokens() {
        let mut entry = entry();
        entry.terminal_title = Some("⠋ raw title".into());
        entry.terminal_title_stripped = Some("raw title".into());
        entry
            .tokens
            .insert("terminal_title".into(), "custom title".into());
        let config = AgentsSidebarConfig {
            rows: vec![vec![
                AgentSidebarToken::TerminalTitle,
                AgentSidebarToken::TerminalTitleStripped,
                AgentSidebarToken::Custom("terminal_title".into()),
            ]],
            ..Default::default()
        };

        assert_eq!(
            agent_rows(&config, &entry, "working"),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle("⠋ raw title".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle("raw title".into())),
                ResolvedToken::unstyled(ResolvedTokenKind::Custom("custom title".into())),
            ]]
        );
    }

    #[test]
    fn known_agent_override_replaces_default_rows() {
        let mut config = AgentsSidebarConfig {
            rows: vec![vec![AgentSidebarToken::Workspace]],
            ..Default::default()
        };
        config
            .rows_by_agent
            .insert("pi".into(), vec![vec![AgentSidebarToken::Agent]]);
        let mut pi = entry();
        pi.agent_label = Some("renamed pi".into());

        assert_eq!(
            agent_rows(&config, &pi, "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Agent(
                "renamed pi".into()
            ))]]
        );

        pi.agent = None;
        assert_eq!(
            agent_rows(&config, &pi, "working"),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Workspace(
                "repo".into()
            ))]]
        );
    }

    #[test]
    fn need_edge_and_waiting_resolve_from_agent_need_state() {
        let mut entry = entry();
        entry.state = AgentState::Idle;
        entry.seen = false;
        entry.last_agent_state_change_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(725));
        let config = AgentsSidebarConfig {
            rows: vec![
                vec![AgentSidebarToken::NeedEdge, AgentSidebarToken::Workspace],
                vec![AgentSidebarToken::NeedEdge, AgentSidebarToken::Waiting],
            ],
            ..Default::default()
        };

        let rows = agent_rows(&config, &entry, "done");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0][0],
            ResolvedToken::unstyled(ResolvedTokenKind::NeedEdge {
                state: AgentState::Idle,
                seen: false,
            })
        );
        assert_eq!(
            rows[1][1],
            ResolvedToken::unstyled(ResolvedTokenKind::Waiting {
                text: "waiting 12m".into(),
                state: AgentState::Idle,
            })
        );

        // A working agent keeps its edge slot (blank in the renderer) but the
        // waiting row elides entirely.
        entry.state = AgentState::Working;
        entry.seen = true;
        let rows = agent_rows(&config, &entry, "working");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0][0],
            ResolvedToken::unstyled(ResolvedTokenKind::NeedEdge {
                state: AgentState::Working,
                seen: true,
            })
        );
    }

    #[test]
    fn waiting_text_scales_by_duration() {
        assert_eq!(
            waiting_text(std::time::Duration::from_secs(45)),
            "waiting 45s"
        );
        assert_eq!(
            waiting_text(std::time::Duration::from_secs(725)),
            "waiting 12m"
        );
        assert_eq!(
            waiting_text(std::time::Duration::from_secs(3600 + 720)),
            "waiting 1h 12m"
        );
    }

    #[test]
    fn grouped_children_suppress_all_builtin_git_details() {
        let config = SpacesSidebarConfig::default();

        assert_eq!(
            space_rows(
                &config,
                SpaceTokenContext {
                    workspace: "feature",
                    branch: Some("worktree/feature"),
                    state_text: "idle",
                    ahead_behind: Some((2, 1)),
                    tokens: &std::collections::HashMap::new(),
                    suppress_git_details: true,
                    agents: (0, 0),
                },
            ),
            vec![vec![
                ResolvedToken::unstyled(ResolvedTokenKind::StateIcon),
                ResolvedToken::unstyled(ResolvedTokenKind::Workspace("feature".into())),
            ]]
        );
    }

    #[test]
    fn workspace_custom_token_can_replace_git_specific_details() {
        let tokens = std::collections::HashMap::from([("jj_status".into(), "2 changes".into())]);
        let config = SpacesSidebarConfig {
            rows: vec![vec![SpaceSidebarToken::Custom("jj_status".into())]],
            ..Default::default()
        };

        assert_eq!(
            space_rows(
                &config,
                SpaceTokenContext {
                    workspace: "repo",
                    branch: None,
                    state_text: "idle",
                    ahead_behind: None,
                    tokens: &tokens,
                    suppress_git_details: false,
                    agents: (0, 0),
                },
            ),
            vec![vec![ResolvedToken::unstyled(ResolvedTokenKind::Custom(
                "2 changes".into()
            ))]]
        );
    }

    fn agents_config() -> SpacesSidebarConfig {
        SpacesSidebarConfig {
            rows: vec![vec![SpaceSidebarToken::Agents]],
            ..Default::default()
        }
    }

    fn agents_context(agents: (usize, usize)) -> SpaceTokenContext<'static> {
        SpaceTokenContext {
            workspace: "repo",
            branch: None,
            state_text: "idle",
            ahead_behind: None,
            tokens: EMPTY_TOKENS.get_or_init(std::collections::HashMap::new),
            suppress_git_details: false,
            agents,
        }
    }

    static EMPTY_TOKENS: std::sync::OnceLock<std::collections::HashMap<String, String>> =
        std::sync::OnceLock::new();

    #[test]
    fn agents_token_elides_when_the_space_has_no_agents() {
        assert_eq!(
            space_rows(&agents_config(), agents_context((0, 0))),
            Vec::<Vec<ResolvedToken>>::new()
        );
    }

    #[test]
    fn agents_token_summarizes_a_count_with_no_attention_needed() {
        assert_eq!(
            space_rows(&agents_config(), agents_context((3, 0))),
            vec![vec![ResolvedToken::unstyled(
                ResolvedTokenKind::SpaceAgents("3 agents".into())
            )]]
        );
    }

    #[test]
    fn agents_token_appends_the_needs_you_count() {
        assert_eq!(
            space_rows(&agents_config(), agents_context((3, 1))),
            vec![vec![ResolvedToken::unstyled(
                ResolvedTokenKind::SpaceAgents("3 agents · 1 need you".into())
            )]]
        );
    }

    #[test]
    fn agents_token_uses_the_singular_noun_for_one_agent() {
        assert_eq!(
            space_rows(&agents_config(), agents_context((1, 1))),
            vec![vec![ResolvedToken::unstyled(
                ResolvedTokenKind::SpaceAgents("1 agent · 1 need you".into())
            )]]
        );
    }

    #[test]
    fn agent_summary_text_covers_singular_plural_and_empty() {
        assert_eq!(agent_summary_text(0, 0), None);
        assert_eq!(agent_summary_text(1, 0).as_deref(), Some("1 agent"));
        assert_eq!(agent_summary_text(2, 0).as_deref(), Some("2 agents"));
        assert_eq!(
            agent_summary_text(4, 2).as_deref(),
            Some("4 agents · 2 need you")
        );
    }
}
