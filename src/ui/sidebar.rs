mod tokens;

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use self::tokens::{ResolvedToken, ResolvedTokenKind, SpaceTokenContext};
use super::scrollbar::{render_scrollbar, should_show_scrollbar};
use super::status::{state_icon, state_label, state_label_color};
use super::text::{display_width, display_width_u16, truncate_end};
use crate::app::state::{AgentPanelSort, Palette};
use crate::app::{AppState, Mode};
use crate::detect::AgentState;
use crate::terminal::TerminalRuntimeRegistry;

const WORKSPACE_SECTION_HEADER_ROWS: u16 = 2;
const AGENT_PANEL_HEADER_ROWS: u16 = 3;

pub(crate) struct AgentPanelEntry {
    pub ws_idx: usize,
    pub tab_idx: usize,
    pub pane_id: crate::layout::PaneId,
    pub primary_label: String,
    pub primary_tab_label: Option<String>,
    pub pane_label: Option<String>,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub agent_label: Option<String>,
    pub agent_kind_label: Option<String>,
    pub agent: Option<crate::detect::Agent>,
    pub state: AgentState,
    pub seen: bool,
    pub last_agent_state_change_seq: Option<u64>,
    pub last_agent_state_change_at: Option<std::time::Instant>,
    pub state_labels: std::collections::HashMap<String, String>,
    pub tokens: std::collections::HashMap<String, String>,
}

fn sidebar_section_heights(total_h: u16, split_ratio: f32) -> (u16, u16) {
    if total_h == 0 {
        return (0, 0);
    }

    if total_h < 6 {
        let ws_h = total_h.div_ceil(2);
        return (ws_h, total_h.saturating_sub(ws_h));
    }

    let ratio = split_ratio.clamp(0.1, 0.9);
    let ws_h = ((total_h as f32) * ratio).round() as u16;
    let ws_h = ws_h.clamp(3, total_h.saturating_sub(3));
    let detail_h = total_h.saturating_sub(ws_h);
    (ws_h, detail_h)
}

pub(crate) fn expanded_sidebar_sections(area: Rect, split_ratio: f32) -> (Rect, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), Rect::default());
    }

    let (ws_h, detail_h) = sidebar_section_heights(content.height, split_ratio);
    let ws_area = Rect::new(content.x, content.y, content.width, ws_h);
    let detail_area = Rect::new(content.x, content.y + ws_h, content.width, detail_h);
    (ws_area, detail_area)
}

pub(crate) fn sidebar_section_divider_rect(area: Rect, split_ratio: f32) -> Rect {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height < 6 {
        return Rect::default();
    }

    let (ws_h, _) = sidebar_section_heights(content.height, split_ratio);
    Rect::new(content.x, content.y + ws_h, content.width, 1)
}

fn agent_panel_sort_label(sort: AgentPanelSort) -> &'static str {
    match sort {
        AgentPanelSort::Spaces => "grouped",
        AgentPanelSort::Priority => "priority",
    }
}

pub(crate) fn agent_panel_toggle_rect(area: Rect, sort: AgentPanelSort) -> Rect {
    agent_panel_header_label_rect(area, agent_panel_sort_label(sort))
}

fn agent_panel_header_label_rect(area: Rect, label: &str) -> Rect {
    if area.width == 0 || area.height < 2 {
        return Rect::default();
    }

    let width = display_width_u16(label).min(area.width);
    Rect::new(
        area.x + area.width.saturating_sub(width),
        area.y + 1,
        width,
        1,
    )
}

fn active_agent_view_label(app: &AppState) -> Option<&str> {
    app.agent_view_override
        .as_ref()
        .map(|view| view.label.as_deref().unwrap_or("filtered"))
}

pub(crate) fn agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, None)
}

pub(crate) fn all_agent_panel_entries(app: &AppState) -> Vec<AgentPanelEntry> {
    collect_agent_panel_entries_with_runtimes(app, None)
}

pub(crate) fn agent_panel_entries_from(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
) -> Vec<AgentPanelEntry> {
    agent_panel_entries_with_runtimes(app, Some(terminal_runtimes))
}

fn agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
) -> Vec<AgentPanelEntry> {
    let mut entries = collect_agent_panel_entries_with_runtimes(app, terminal_runtimes);
    crate::app::agent_view::apply_agent_view(app, &mut entries);
    entries
}

fn collect_agent_panel_entries_with_runtimes(
    app: &AppState,
    terminal_runtimes: Option<&TerminalRuntimeRegistry>,
) -> Vec<AgentPanelEntry> {
    let empty_runtimes;
    let terminal_runtimes = match terminal_runtimes {
        Some(terminal_runtimes) => terminal_runtimes,
        None => {
            empty_runtimes = TerminalRuntimeRegistry::new();
            &empty_runtimes
        }
    };

    app.workspaces
        .iter()
        .enumerate()
        .flat_map(|(ws_idx, ws)| {
            let multi_tab = ws.tabs.len() > 1;
            let workspace_label = ws.display_name_from(&app.terminals, terminal_runtimes);
            ws.pane_details(&app.terminals)
                .into_iter()
                .map(move |detail| {
                    let show_tab = multi_tab
                        || ws
                            .tabs
                            .get(detail.tab_idx)
                            .is_some_and(|tab| !tab.is_auto_named());
                    AgentPanelEntry {
                        ws_idx,
                        tab_idx: detail.tab_idx,
                        pane_id: detail.pane_id,
                        primary_label: workspace_label.clone(),
                        primary_tab_label: show_tab.then_some(detail.tab_label),
                        pane_label: detail.pane_label,
                        terminal_title: detail.terminal_title,
                        terminal_title_stripped: detail.terminal_title_stripped,
                        agent_label: Some(detail.agent_label),
                        agent_kind_label: detail.agent_kind_label,
                        agent: detail.agent,
                        state: detail.state,
                        seen: detail.seen,
                        last_agent_state_change_seq: detail.last_agent_state_change_seq,
                        last_agent_state_change_at: detail.last_agent_state_change_at,
                        state_labels: detail.state_labels,
                        tokens: detail.tokens,
                    }
                })
        })
        .collect()
}

pub(super) fn agent_panel_status_key(state: AgentState, seen: bool) -> &'static str {
    match (state, seen) {
        (AgentState::Idle, false) => "done",
        (AgentState::Idle, true) => "idle",
        (AgentState::Working, _) => "working",
        (AgentState::Blocked, _) => "blocked",
        (AgentState::Unknown, _) => "unknown",
    }
}

/// (total known agents in a space, how many need attention). A pane counts as an
/// agent when it resolves to a known agent kind; it needs attention when it is
/// blocked or finished-and-unseen (the same rule the agent panel uses). Drives the
/// `agents` space token's "N agents · M need you" summary.
fn workspace_agent_summary(app: &AppState, ws: &crate::workspace::Workspace) -> (usize, usize) {
    ws.pane_details(&app.terminals)
        .iter()
        .filter(|detail| detail.agent.is_some())
        .fold((0usize, 0usize), |(total, needs_you), detail| {
            (
                total + 1,
                needs_you + usize::from(tokens::needs_attention(detail.state, detail.seen)),
            )
        })
}

fn workspace_row_height(app: &AppState, ws: &crate::workspace::Workspace, indented: bool) -> u16 {
    let (state, seen) = ws.aggregate_state(&app.terminals);
    let label = if indented {
        grouped_child_display_label(
            &ws.display_name_from_terminals(&app.terminals),
            ws.branch().as_deref(),
            ws.custom_name.is_some(),
        )
    } else {
        ws.display_name_from_terminals(&app.terminals)
    };
    let token_values = ws.metadata_tokens.values();
    tokens::space_rows(
        &app.sidebar_spaces,
        SpaceTokenContext {
            workspace: &label,
            branch: ws.branch().as_deref(),
            state_text: state_label(state, seen),
            ahead_behind: ws.git_ahead_behind(),
            tokens: &token_values,
            suppress_git_details: indented,
            agents: workspace_agent_summary(app, ws),
        },
    )
    .len()
    .max(1)
    .min(u16::MAX as usize) as u16
}

fn workspace_row_height_in_body(
    app: &AppState,
    workspace: &crate::workspace::Workspace,
    indented: bool,
    body_height: u16,
) -> u16 {
    workspace_row_height(app, workspace, indented).min(body_height)
}

fn workspace_attention_priority(state: AgentState, seen: bool) -> u8 {
    match (state, seen) {
        (AgentState::Blocked, _) => 4,
        (AgentState::Idle, false) => 3,
        (AgentState::Working, _) => 2,
        (AgentState::Idle, true) => 1,
        (AgentState::Unknown, _) => 0,
    }
}

fn space_aggregate_state(app: &AppState, key: &str) -> (AgentState, bool) {
    app.workspaces
        .iter()
        .filter(|ws| ws.worktree_space().is_some_and(|space| space.key == key))
        .map(|ws| ws.aggregate_state(&app.terminals))
        .max_by_key(|(state, seen)| workspace_attention_priority(*state, *seen))
        .unwrap_or((AgentState::Unknown, true))
}

pub(crate) fn workspace_parent_group_state(
    app: &AppState,
    ws_idx: usize,
) -> Option<(String, bool)> {
    let space = app.workspaces.get(ws_idx)?.worktree_space()?;
    if space.is_linked_worktree {
        return None;
    }
    let member_count = app
        .workspaces
        .iter()
        .filter(|ws| {
            ws.worktree_space()
                .is_some_and(|member| member.key == space.key)
        })
        .count();
    (member_count >= 2).then(|| {
        (
            space.key.clone(),
            app.collapsed_space_keys.contains(&space.key),
        )
    })
}

pub(crate) fn grouped_child_display_label(
    label: &str,
    branch: Option<&str>,
    has_custom_name: bool,
) -> String {
    if has_custom_name {
        return label.to_string();
    }
    let Some(branch) = branch else {
        return label.to_string();
    };
    branch
        .strip_prefix("worktree/")
        .unwrap_or(branch)
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceListEntry {
    Workspace { ws_idx: usize, indented: bool },
}

/// Collapse key for a spaces tag group. Kept distinct from worktree-space keys,
/// which use their raw repo key, so the two grouping mechanisms never collide.
pub(crate) fn tag_collapse_key(tag: &str) -> String {
    format!("tag:{tag}")
}

/// Collapse key for an agents-panel tag group. Kept distinct from the spaces
/// `tag:<name>` key so a tag can be collapsed in one panel and open in the other.
pub(crate) fn agent_tag_collapse_key(tag: &str) -> String {
    format!("agent-tag:{tag}")
}

/// Rolled-up attention level for a group of workspaces, most-urgent first. Used
/// to color the leading edge of a tag group header from its members' states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NeedLevel {
    /// A member is blocked and needs a human.
    Blocked,
    /// A member finished and has not been seen yet (idle + unseen).
    NeedsYou,
    /// A member is actively working.
    Working,
    /// A member is idle and already seen — nothing pending, but present.
    Idle,
}

fn need_level_priority(need: NeedLevel) -> u8 {
    match need {
        NeedLevel::Blocked => 3,
        NeedLevel::NeedsYou => 2,
        NeedLevel::Working => 1,
        NeedLevel::Idle => 0,
    }
}

/// Reduce each member's `(state, seen)` to the strongest attention level across
/// the group, ordered Blocked > NeedsYou > Working > Idle. An empty slice, or a
/// group whose members are all Unknown, yields `None` (no edge to draw).
pub(crate) fn need_rollup(states: &[(AgentState, bool)]) -> Option<NeedLevel> {
    states
        .iter()
        .filter_map(|(state, seen)| match (state, seen) {
            (AgentState::Blocked, _) => Some(NeedLevel::Blocked),
            (AgentState::Idle, false) => Some(NeedLevel::NeedsYou),
            (AgentState::Working, _) => Some(NeedLevel::Working),
            (AgentState::Idle, true) => Some(NeedLevel::Idle),
            (AgentState::Unknown, _) => None,
        })
        .max_by_key(|need| need_level_priority(*need))
}

/// One ordered tag group: the tag name and the top-level entry indices that
/// belong to it, in their original list order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TagGroup {
    pub tag: String,
    pub member_entry_indices: Vec<usize>,
}

/// Grouping of a top-level entry list by tag. `groups` are in first-appearance
/// tag order; `ungrouped` are the remaining top-level entry indices in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TagGrouping {
    pub groups: Vec<TagGroup>,
    pub ungrouped: Vec<usize>,
}

/// Pure grouping core: given each top-level entry's optional tag (in list order),
/// return the tag groups in first-appearance order plus the untagged remainder.
/// A tag with a single member still forms a group so its header is always shown.
pub(crate) fn group_entries_by_tag(entry_tags: &[Option<String>]) -> TagGrouping {
    let mut order: Vec<String> = Vec::new();
    let mut members: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    let mut ungrouped: Vec<usize> = Vec::new();

    for (idx, tag) in entry_tags.iter().enumerate() {
        match tag {
            Some(tag) if !tag.is_empty() => {
                if !members.contains_key(tag) {
                    order.push(tag.clone());
                }
                members.entry(tag.clone()).or_default().push(idx);
            }
            _ => ungrouped.push(idx),
        }
    }

    let groups = order
        .into_iter()
        .map(|tag| {
            let member_entry_indices = members.remove(&tag).unwrap_or_default();
            TagGroup {
                tag,
                member_entry_indices,
            }
        })
        .collect();

    TagGrouping { groups, ungrouped }
}

/// Reorder tag groups in place per the sort mode. `Manual` follows `manual_order`
/// (tags absent from it keep their first-appearance order, appended after the
/// listed ones); `Name` sorts case-insensitively by tag name; `FirstAppearance`
/// leaves the input order untouched. Sorting is stable, so ties (and unknown
/// tags under `Manual`) preserve first-appearance order.
pub(crate) fn sort_tag_groups(
    groups: &mut [TagGroup],
    mode: crate::config::TagSortMode,
    manual_order: &[String],
) {
    match mode {
        crate::config::TagSortMode::FirstAppearance => {}
        crate::config::TagSortMode::Name => {
            groups.sort_by_key(|group| group.tag.to_lowercase());
        }
        crate::config::TagSortMode::Manual => {
            let rank = |tag: &str| {
                manual_order
                    .iter()
                    .position(|listed| listed == tag)
                    .unwrap_or(usize::MAX)
            };
            groups.sort_by_key(|group| rank(&group.tag));
        }
    }
}

/// The tag-group names in the spaces list's display order, respecting the active
/// sort mode and manual order. Drives header-menu "is first / is last" and the
/// move-up/down reorder. Empty when no workspace carries a tag.
pub(crate) fn ordered_tag_names(app: &AppState) -> Vec<String> {
    tag_layout_rows(app)
        .into_iter()
        .filter_map(|row| match row {
            TagLayoutRow::Header { tag, .. } => Some(tag),
            TagLayoutRow::Entry { .. } => None,
        })
        .collect()
}

pub(crate) fn next_entry_is_indented_workspace(entries: &[WorkspaceListEntry], idx: usize) -> bool {
    matches!(
        entries.get(idx.saturating_add(1)),
        Some(WorkspaceListEntry::Workspace { indented: true, .. })
    )
}

pub(crate) fn normalized_workspace_scroll(app: &AppState, area: Rect, requested: usize) -> usize {
    let ws_area = workspace_list_rect(area, app.sidebar_section_split);
    let body = workspace_list_body_rect(ws_area, false);
    if body.height == 0 {
        return requested;
    }

    if workspace_list_entries(app).is_empty() {
        0
    } else {
        requested.min(workspace_list_bottom_start(app, ws_area))
    }
}

pub(crate) fn workspace_list_entries(app: &AppState) -> Vec<WorkspaceListEntry> {
    workspace_list_entries_inner(app, false)
}

/// Like [`workspace_list_entries`] but always expands worktree groups, ignoring
/// `collapsed_space_keys`. The mobile switcher has no collapse affordance and
/// always shows the full worktree tree.
pub(crate) fn workspace_list_entries_expanded(app: &AppState) -> Vec<WorkspaceListEntry> {
    workspace_list_entries_inner(app, true)
}

fn workspace_list_entries_inner(app: &AppState, force_expanded: bool) -> Vec<WorkspaceListEntry> {
    let mut members_by_key = std::collections::HashMap::<String, Vec<usize>>::new();
    for (ws_idx, ws) in app.workspaces.iter().enumerate() {
        if let Some(space) = ws.worktree_space() {
            members_by_key
                .entry(space.key.clone())
                .or_default()
                .push(ws_idx);
        }
    }
    let grouped_keys = members_by_key
        .iter()
        .filter(|(_, members)| {
            members.len() >= 2
                && members.iter().any(|idx| {
                    app.workspaces
                        .get(*idx)
                        .and_then(|ws| ws.worktree_space())
                        .is_some_and(|space| !space.is_linked_worktree)
                })
        })
        .map(|(key, _)| key.clone())
        .collect::<std::collections::HashSet<_>>();

    let visible_group_idx = if matches!(app.mode, Mode::Navigate) {
        Some(app.selected)
    } else {
        app.active
    };
    let active_group = visible_group_idx.and_then(|idx| {
        app.workspaces
            .get(idx)
            .and_then(|ws| ws.worktree_space())
            .map(|space| space.key.clone())
    });

    let mut emitted_groups = std::collections::HashSet::<String>::new();
    let mut entries = Vec::new();
    for (ws_idx, ws) in app.workspaces.iter().enumerate() {
        let Some(space) = ws
            .worktree_space()
            .filter(|space| grouped_keys.contains(&space.key))
        else {
            entries.push(WorkspaceListEntry::Workspace {
                ws_idx,
                indented: false,
            });
            continue;
        };

        if !emitted_groups.insert(space.key.clone()) {
            continue;
        }

        let Some(members) = members_by_key.get(&space.key) else {
            continue;
        };
        let Some(parent_idx) = members.iter().copied().find(|idx| {
            app.workspaces
                .get(*idx)
                .and_then(|member| member.worktree_space())
                .is_some_and(|member_space| !member_space.is_linked_worktree)
        }) else {
            entries.push(WorkspaceListEntry::Workspace {
                ws_idx,
                indented: false,
            });
            continue;
        };
        let collapsed = !force_expanded && app.collapsed_space_keys.contains(&space.key);
        entries.push(WorkspaceListEntry::Workspace {
            ws_idx: parent_idx,
            indented: false,
        });

        if collapsed {
            if let Some(active_idx) = visible_group_idx
                .filter(|idx| *idx != parent_idx)
                .filter(|_| active_group.as_deref() == Some(space.key.as_str()))
            {
                entries.push(WorkspaceListEntry::Workspace {
                    ws_idx: active_idx,
                    indented: true,
                });
            }
        } else {
            for member_idx in members {
                if *member_idx == parent_idx {
                    continue;
                }
                entries.push(WorkspaceListEntry::Workspace {
                    ws_idx: *member_idx,
                    indented: true,
                });
            }
        }
    }
    entries
}

/// A desktop sidebar row: either a tag group header or a workspace entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TagLayoutRow {
    Header {
        tag: String,
        count: usize,
        collapsed: bool,
        /// First workspace of the group; the header card keys off it so the tag
        /// name stays resolvable even when the group is collapsed.
        first_ws_idx: usize,
    },
    Entry {
        entry: WorkspaceListEntry,
        /// True when this entry belongs under a tag group header, so its card
        /// indents two cells beneath the header. Untagged rows stay at column 0.
        grouped: bool,
    },
}

/// Split a worktree-grouped entry list into top-level blocks, where each block is
/// a top-level entry followed by its indented worktree children.
fn top_level_blocks(entries: &[WorkspaceListEntry]) -> Vec<Vec<WorkspaceListEntry>> {
    let mut blocks: Vec<Vec<WorkspaceListEntry>> = Vec::new();
    for entry in entries {
        match entry {
            WorkspaceListEntry::Workspace {
                indented: false, ..
            } => blocks.push(vec![entry.clone()]),
            WorkspaceListEntry::Workspace { indented: true, .. } => {
                if let Some(last) = blocks.last_mut() {
                    last.push(entry.clone());
                } else {
                    blocks.push(vec![entry.clone()]);
                }
            }
        }
    }
    blocks
}

/// Desktop layout: group the top-level workspace blocks by tag, ordering tagged
/// groups first (in first-appearance order) with a header before each, and the
/// untagged blocks after. A collapsed tag group hides its member rows but keeps
/// its header. Worktree grouping inside a block is preserved untouched.
pub(crate) fn tag_layout_rows(app: &AppState) -> Vec<TagLayoutRow> {
    let entries = workspace_list_entries(app);
    let blocks = top_level_blocks(&entries);

    let block_tags: Vec<Option<String>> = blocks
        .iter()
        .map(|block| {
            block.first().and_then(|entry| {
                let WorkspaceListEntry::Workspace { ws_idx, .. } = entry;
                app.workspaces
                    .get(*ws_idx)
                    .and_then(|ws| ws.tag().map(str::to_string))
            })
        })
        .collect();

    let mut grouping = group_entries_by_tag(&block_tags);
    sort_tag_groups(
        &mut grouping.groups,
        app.sidebar_spaces.tag_sort,
        &app.tag_order,
    );
    let mut rows = Vec::new();

    for group in &grouping.groups {
        let collapsed = app
            .collapsed_space_keys
            .contains(&tag_collapse_key(&group.tag));
        let first_ws_idx = group
            .member_entry_indices
            .first()
            .and_then(|block_idx| blocks[*block_idx].first())
            .map(|entry| {
                let WorkspaceListEntry::Workspace { ws_idx, .. } = entry;
                *ws_idx
            })
            .unwrap_or(0);
        rows.push(TagLayoutRow::Header {
            tag: group.tag.clone(),
            count: group.member_entry_indices.len(),
            collapsed,
            first_ws_idx,
        });
        if collapsed {
            continue;
        }
        for block_idx in &group.member_entry_indices {
            for entry in &blocks[*block_idx] {
                rows.push(TagLayoutRow::Entry {
                    entry: entry.clone(),
                    grouped: true,
                });
            }
        }
    }

    for block_idx in &grouping.ungrouped {
        for entry in &blocks[*block_idx] {
            rows.push(TagLayoutRow::Entry {
                entry: entry.clone(),
                grouped: false,
            });
        }
    }

    rows
}

/// True when any workspace carries a tag, so the sidebar should group by tag.
pub(crate) fn has_tag_groups(app: &AppState) -> bool {
    app.workspaces.iter().any(|ws| ws.tag().is_some())
}

/// Workspace indices in the order the desktop sidebar draws them, tag headers
/// excluded and collapsed-group members omitted. Drives up/down navigation.
pub(crate) fn workspace_display_order(app: &AppState) -> Vec<usize> {
    workspace_display_rows(app)
        .into_iter()
        .filter_map(|row| match row {
            TagLayoutRow::Entry {
                entry: WorkspaceListEntry::Workspace { ws_idx, .. },
                ..
            } => Some(ws_idx),
            TagLayoutRow::Header { .. } => None,
        })
        .collect()
}

/// Display-row index of a top-level or child workspace, or `None` when it is
/// hidden inside a collapsed group. `app.workspace_scroll` indexes this list.
pub(crate) fn workspace_display_row_index(app: &AppState, ws_idx: usize) -> Option<usize> {
    workspace_display_rows(app).iter().position(|row| {
        matches!(
            row,
            TagLayoutRow::Entry {
                entry: WorkspaceListEntry::Workspace { ws_idx: entry_idx, .. },
                ..
            } if *entry_idx == ws_idx
        )
    })
}

/// The ordered rows the desktop workspace list draws: tag headers interleaved
/// with workspace entries when any workspace is tagged, or the plain worktree
/// entry list otherwise. `app.workspace_scroll` indexes into this list.
fn workspace_display_rows(app: &AppState) -> Vec<TagLayoutRow> {
    if has_tag_groups(app) {
        tag_layout_rows(app)
    } else {
        workspace_list_entries(app)
            .into_iter()
            .map(|entry| TagLayoutRow::Entry {
                entry,
                grouped: false,
            })
            .collect()
    }
}

/// Height of a single display row within the body, headers being one row tall.
fn display_row_height(app: &AppState, row: &TagLayoutRow, body_height: u16) -> u16 {
    match row {
        TagLayoutRow::Header { .. } => 1u16.min(body_height),
        TagLayoutRow::Entry {
            entry: WorkspaceListEntry::Workspace { ws_idx, indented },
            ..
        } => app
            .workspaces
            .get(*ws_idx)
            .map(|ws| workspace_row_height_in_body(app, ws, *indented, body_height))
            .unwrap_or(0),
    }
}

/// Row gap after a display row. Headers and the row before an indented child sit
/// flush; every other row carries the configured spaces gap.
fn display_row_gap(app: &AppState, rows: &[TagLayoutRow], idx: usize) -> u16 {
    let next_is_indented_child = matches!(
        rows.get(idx.saturating_add(1)),
        Some(TagLayoutRow::Entry {
            entry: WorkspaceListEntry::Workspace { indented: true, .. },
            ..
        })
    );
    let is_header = matches!(rows.get(idx), Some(TagLayoutRow::Header { .. }));
    if idx + 1 < rows.len() && !next_is_indented_child && !is_header {
        app.sidebar_spaces.row_gap
    } else {
        0
    }
}

pub(crate) fn workspace_list_rect(area: Rect, split_ratio: f32) -> Rect {
    let (ws_area, _) = expanded_sidebar_sections(area, split_ratio);
    ws_area
}

pub(crate) fn workspace_list_body_rect(area: Rect, has_scrollbar: bool) -> Rect {
    if area.width == 0 || area.height <= WORKSPACE_SECTION_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(WORKSPACE_SECTION_HEADER_ROWS);
    let footer_y = area.y + area.height.saturating_sub(1);
    let body_height = footer_y.saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn workspace_list_visible_count(app: &AppState, area: Rect, scroll: usize) -> usize {
    let body = workspace_list_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    let rows = workspace_display_rows(app);
    for (row_idx, row) in rows.iter().enumerate().skip(scroll) {
        let row_height = display_row_height(app, row, body.height);
        let gap = display_row_gap(app, &rows, row_idx);
        if used_rows.saturating_add(row_height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(row_height);
        visible += 1;
        used_rows = used_rows.saturating_add(gap).min(body.height);
    }
    visible
}

fn workspace_list_bottom_start(app: &AppState, area: Rect) -> usize {
    let body = workspace_list_body_rect(area, false);
    let rows = workspace_display_rows(app);
    let mut used_rows = 0u16;
    let mut start = rows.len();
    for (row_idx, row) in rows.iter().enumerate().rev() {
        let gap = display_row_gap(app, &rows, row_idx);
        let needed = display_row_height(app, row, body.height).saturating_add(gap);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        start = row_idx;
    }
    start.min(rows.len().saturating_sub(1))
}

pub(crate) fn workspace_list_scroll_metrics(
    app: &AppState,
    area: Rect,
) -> crate::pane::ScrollMetrics {
    let max_scroll = workspace_list_bottom_start(app, area);
    let scroll = app.workspace_scroll.min(max_scroll);
    let viewport_rows = workspace_list_visible_count(app, area, scroll);

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_scroll.saturating_sub(scroll),
        max_offset_from_bottom: max_scroll,
        viewport_rows,
    }
}

pub(crate) fn workspace_list_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    let metrics = workspace_list_scroll_metrics(app, area);
    let body = workspace_list_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn agent_panel_body_rect(area: Rect, has_scrollbar: bool) -> Rect {
    if area.width == 0 || area.height <= AGENT_PANEL_HEADER_ROWS {
        return Rect::default();
    }

    let body_y = area.y.saturating_add(AGENT_PANEL_HEADER_ROWS);
    let body_height = (area.y + area.height).saturating_sub(body_y);
    let body_width = area.width.saturating_sub(u16::from(has_scrollbar));
    Rect::new(area.x, body_y, body_width, body_height)
}

fn resolved_agent_rows(app: &AppState, entry: &AgentPanelEntry) -> Vec<Vec<ResolvedToken>> {
    let label = entry
        .state_labels
        .get(agent_panel_status_key(entry.state, entry.seen))
        .map(String::as_str)
        .unwrap_or_else(|| state_label(entry.state, entry.seen));
    tokens::agent_rows(&app.sidebar_agents, entry, label)
}

pub(crate) fn agent_entry_height_in_body(
    app: &AppState,
    entry: &AgentPanelEntry,
    body_height: u16,
) -> u16 {
    (resolved_agent_rows(app, entry)
        .len()
        .max(1)
        .min(u16::MAX as usize) as u16)
        .min(body_height)
}

/// A display row in the agents panel: either a tag group header or an agent
/// entry. Entries carry the index into [`agent_panel_entries`] so hit-testing
/// resolves back to the same `AgentPanelEntry` the panel drew.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentPanelRow {
    Header {
        tag: String,
        count: usize,
        collapsed: bool,
        /// Rolled-up attention across the group's member agents, most-urgent
        /// first, driving the header's leading edge exactly as the spaces
        /// headers color theirs.
        need: Option<NeedLevel>,
    },
    Entry {
        /// Index into [`agent_panel_entries`].
        entry_idx: usize,
    },
}

/// True when any agent-panel entry inherits a tag from its workspace, so the
/// panel should group its rows under tag headers.
pub(crate) fn has_agent_tag_groups(app: &AppState, entries: &[AgentPanelEntry]) -> bool {
    entries.iter().any(|entry| {
        app.workspaces
            .get(entry.ws_idx)
            .and_then(|ws| ws.tag())
            .is_some()
    })
}

/// Group the agent-panel entries by their inherited workspace tag, interleaving
/// a one-row header before each group (first-appearance tag order) with the
/// untagged entries listed after, mirroring the spaces list. An agent is never
/// tagged directly: it inherits its workspace's tag. When no entry carries a
/// tag, this is the plain entry list with no headers.
pub(crate) fn agent_panel_display_rows(
    app: &AppState,
    entries: &[AgentPanelEntry],
) -> Vec<AgentPanelRow> {
    if !has_agent_tag_groups(app, entries) {
        return (0..entries.len())
            .map(|entry_idx| AgentPanelRow::Entry { entry_idx })
            .collect();
    }

    let entry_tags: Vec<Option<String>> = entries
        .iter()
        .map(|entry| {
            app.workspaces
                .get(entry.ws_idx)
                .and_then(|ws| ws.tag().map(str::to_string))
        })
        .collect();
    let mut grouping = group_entries_by_tag(&entry_tags);
    sort_tag_groups(
        &mut grouping.groups,
        app.sidebar_spaces.tag_sort,
        &app.tag_order,
    );

    let mut rows = Vec::new();
    for group in &grouping.groups {
        let collapsed = app
            .collapsed_space_keys
            .contains(&agent_tag_collapse_key(&group.tag));
        let member_states: Vec<(AgentState, bool)> = group
            .member_entry_indices
            .iter()
            .filter_map(|entry_idx| entries.get(*entry_idx))
            .map(|entry| (entry.state, entry.seen))
            .collect();
        rows.push(AgentPanelRow::Header {
            tag: group.tag.clone(),
            count: group.member_entry_indices.len(),
            collapsed,
            need: need_rollup(&member_states),
        });
        if collapsed {
            continue;
        }
        for entry_idx in &group.member_entry_indices {
            rows.push(AgentPanelRow::Entry {
                entry_idx: *entry_idx,
            });
        }
    }
    for entry_idx in &grouping.ungrouped {
        rows.push(AgentPanelRow::Entry {
            entry_idx: *entry_idx,
        });
    }

    rows
}

/// Display-row index of the entry at `entry_idx`, mapping an `agent_panel_entries`
/// index through the interleaved header rows so scroll math (which indexes the
/// display list) can target the right visible row. `None` when the entry is
/// hidden inside a collapsed group or out of range.
pub(crate) fn agent_panel_display_row_for_entry(
    rows: &[AgentPanelRow],
    entry_idx: usize,
) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, AgentPanelRow::Entry { entry_idx: e } if *e == entry_idx))
}

/// Height of one agent-panel display row within the body: a header is one row
/// tall, an entry is its rendered agent-rows height.
pub(crate) fn agent_display_row_height(
    app: &AppState,
    entries: &[AgentPanelEntry],
    row: &AgentPanelRow,
    body_height: u16,
) -> u16 {
    match row {
        AgentPanelRow::Header { .. } => 1u16.min(body_height),
        AgentPanelRow::Entry { entry_idx } => entries
            .get(*entry_idx)
            .map(|entry| agent_entry_height_in_body(app, entry, body_height))
            .unwrap_or(0),
    }
}

/// Row gap after an agent-panel display row. Headers sit flush against the row
/// below them; every other row but the last carries the configured agents gap.
pub(crate) fn agent_display_row_gap(app: &AppState, rows: &[AgentPanelRow], idx: usize) -> u16 {
    let is_header = matches!(rows.get(idx), Some(AgentPanelRow::Header { .. }));
    if idx + 1 < rows.len() && !is_header {
        app.sidebar_agents.row_gap
    } else {
        0
    }
}

fn agent_panel_visible_count_from(app: &AppState, area: Rect, scroll: usize) -> usize {
    let body = agent_panel_body_rect(area, false);
    if body.width == 0 || body.height == 0 {
        return 0;
    }

    let mut used_rows = 0u16;
    let mut visible = 0usize;
    let entries = agent_panel_entries(app);
    let rows = agent_panel_display_rows(app, &entries);
    for (index, row) in rows.iter().enumerate().skip(scroll) {
        let height = agent_display_row_height(app, &entries, row, body.height);
        if used_rows.saturating_add(height) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(height);
        visible += 1;
        used_rows = used_rows
            .saturating_add(agent_display_row_gap(app, &rows, index))
            .min(body.height);
    }
    visible
}

fn agent_panel_bottom_start(app: &AppState, area: Rect) -> usize {
    let body = agent_panel_body_rect(area, false);
    let entries = agent_panel_entries(app);
    let rows = agent_panel_display_rows(app, &entries);
    let mut used_rows = 0u16;
    let mut start = rows.len();
    for (index, row) in rows.iter().enumerate().rev() {
        let gap = agent_display_row_gap(app, &rows, index);
        let needed = agent_display_row_height(app, &entries, row, body.height).saturating_add(gap);
        if used_rows.saturating_add(needed) > body.height {
            break;
        }
        used_rows = used_rows.saturating_add(needed);
        start = index;
    }
    start.min(rows.len().saturating_sub(1))
}

pub(crate) fn agent_panel_scroll_for_target(
    app: &AppState,
    area: Rect,
    current_scroll: usize,
    target: usize,
) -> usize {
    let max_scroll = agent_panel_bottom_start(app, area);
    if target < current_scroll {
        return target.min(max_scroll);
    }
    let mut scroll = current_scroll.min(max_scroll);
    while scroll < target {
        let visible = agent_panel_visible_count_from(app, area, scroll);
        if visible > 0 && target < scroll.saturating_add(visible) {
            break;
        }
        scroll += 1;
    }
    scroll.min(max_scroll)
}

pub(crate) fn agent_panel_scroll_metrics(app: &AppState, area: Rect) -> crate::pane::ScrollMetrics {
    let max_scroll = agent_panel_bottom_start(app, area);
    let scroll = app.agent_panel_scroll.min(max_scroll);
    let viewport_rows = agent_panel_visible_count_from(app, area, scroll);

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_scroll.saturating_sub(scroll),
        max_offset_from_bottom: max_scroll,
        viewport_rows,
    }
}

pub(crate) fn agent_panel_scrollbar_rect(app: &AppState, area: Rect) -> Option<Rect> {
    let metrics = agent_panel_scroll_metrics(app, area);
    let body = agent_panel_body_rect(area, true);
    (should_show_scrollbar(metrics) && body.width > 0 && body.height > 0).then_some(Rect::new(
        area.x + area.width.saturating_sub(1),
        body.y,
        1,
        body.height,
    ))
}

pub(crate) fn compute_workspace_list_areas(
    app: &AppState,
    area: Rect,
) -> (Vec<crate::app::state::WorkspaceCardArea>, Vec<()>) {
    let ws_area = workspace_list_rect(area, app.sidebar_section_split);
    if ws_area == Rect::default() {
        return (Vec::new(), Vec::new());
    }

    let metrics = workspace_list_scroll_metrics(app, ws_area);
    let body = workspace_list_body_rect(ws_area, should_show_scrollbar(metrics));
    if body.width == 0 || body.height == 0 {
        return (Vec::new(), Vec::new());
    }

    let scroll = app.workspace_scroll;
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let mut cards = Vec::new();
    let headers = Vec::new();

    let rows = workspace_display_rows(app);
    for (row_idx, row) in rows.iter().enumerate().skip(scroll) {
        let (ws_idx, indented, is_tag_header, grouped) = match row {
            TagLayoutRow::Header { first_ws_idx, .. } => (*first_ws_idx, false, true, false),
            TagLayoutRow::Entry {
                entry: WorkspaceListEntry::Workspace { ws_idx, indented },
                grouped,
            } => {
                if app.workspaces.get(*ws_idx).is_none() {
                    continue;
                }
                (*ws_idx, *indented, false, *grouped)
            }
        };
        let row_height = display_row_height(app, row, body.height);
        let gap = display_row_gap(app, &rows, row_idx);
        if row_y.saturating_add(row_height) > body_bottom {
            break;
        }
        // A tag-group member card indents two cells under its header; everything
        // drawn inside the card (state icon, name, rollup edge) rides the shift.
        let (card_x, card_width) = if grouped {
            (body.x.saturating_add(2), body.width.saturating_sub(2))
        } else {
            (body.x, body.width)
        };
        cards.push(crate::app::state::WorkspaceCardArea {
            ws_idx,
            rect: Rect::new(card_x, row_y, card_width, row_height),
            indented,
            is_tag_header,
        });
        row_y = row_y
            .saturating_add(row_height)
            .saturating_add(gap)
            .min(body_bottom);
    }

    (cards, headers)
}

pub(crate) fn compute_workspace_card_areas(
    app: &AppState,
    area: Rect,
) -> Vec<crate::app::state::WorkspaceCardArea> {
    compute_workspace_list_areas(app, area).0
}

pub(crate) fn workspace_group_chevron_rect(card: &crate::app::state::WorkspaceCardArea) -> Rect {
    if card.rect.width == 0 || card.rect.height == 0 {
        return Rect::default();
    }

    Rect::new(
        card.rect.x + card.rect.width.saturating_sub(1),
        card.rect.y,
        1,
        1,
    )
}

/// Auto-scale sidebar width based on workspace identity + agent summary.
pub(crate) fn collapsed_sidebar_sections(area: Rect) -> (Rect, Option<u16>, Rect) {
    let content = Rect::new(area.x, area.y, area.width.saturating_sub(1), area.height);
    if content.width == 0 || content.height == 0 {
        return (Rect::default(), None, Rect::default());
    }

    if content.height < 7 {
        return (content, None, Rect::default());
    }

    let total_h = content.height as usize;
    let ws_h = total_h.div_ceil(2);
    let detail_h = total_h.saturating_sub(ws_h + 1);
    if ws_h == 0 || detail_h == 0 {
        return (content, None, Rect::default());
    }

    let divider_y = content.y + ws_h as u16;
    let ws_area = Rect::new(content.x, content.y, content.width, ws_h as u16);
    let detail_area = Rect::new(content.x, divider_y + 1, content.width, detail_h as u16);
    (ws_area, Some(divider_y), detail_area)
}

fn workspace_selection_background(p: &Palette, is_active: bool) -> Color {
    if is_active && p.selection_bg == Color::Reset {
        p.active_row_bg
    } else {
        p.selection_bg
    }
}

/// Collapsed sidebar: workspace glance on top, compact agent list below.
pub(super) fn render_sidebar_collapsed(app: &AppState, frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let is_navigating = matches!(app.mode, Mode::Navigate);

    let p = &app.palette;
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(p.sidebar_bg));
    let sep_style = if is_navigating {
        Style::default().fg(p.accent)
    } else {
        Style::default().fg(p.surface_dim)
    };
    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let (ws_area, divider_y, detail_area) = collapsed_sidebar_sections(area);
    if ws_area == Rect::default() {
        render_sidebar_toggle(app, frame, area, true, p);
        return;
    }

    for (visible_idx, ws) in app.workspaces.iter().enumerate() {
        let y = ws_area.y + visible_idx as u16;
        if y >= ws_area.y + ws_area.height {
            break;
        }
        let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);
        let (icon, icon_style) = state_icon(agg_state, agg_seen, app.status_indicators, p);
        let is_selected = visible_idx == app.selected && is_navigating;
        let is_active = Some(visible_idx) == app.active;
        let selection_bg = workspace_selection_background(p, is_active);
        let row_style = if is_selected {
            Style::default().bg(selection_bg)
        } else if is_active {
            Style::default().bg(p.active_row_bg)
        } else {
            Style::default()
        };
        let num_style = if is_selected {
            Style::default().fg(p.overlay1).bg(selection_bg)
        } else if is_active {
            Style::default().fg(p.text).bg(p.active_row_bg)
        } else {
            Style::default().fg(p.overlay0)
        };

        if is_selected || is_active {
            let buf = frame.buffer_mut();
            for x in ws_area.x..ws_area.x + ws_area.width {
                buf[(x, y)].set_style(row_style);
            }
        }

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{:<2}", visible_idx + 1), num_style),
                Span::styled(icon, icon_style),
            ])),
            Rect::new(ws_area.x, y, ws_area.width, 1),
        );
    }

    if let Some(divider_y) = divider_y {
        let buf = frame.buffer_mut();
        let divider_color = if app.agent_view_override.is_some() {
            p.accent
        } else {
            p.surface_dim
        };
        for x in ws_area.x..ws_area.x + ws_area.width {
            buf[(x, divider_y)].set_symbol("─");
            buf[(x, divider_y)].set_style(Style::default().fg(divider_color));
        }
    }

    let detail_content_area = Rect::new(
        detail_area.x,
        detail_area.y,
        detail_area.width,
        detail_area.height.saturating_sub(1),
    );
    if detail_content_area != Rect::default() {
        for (detail_idx, detail) in agent_panel_entries(app).iter().enumerate() {
            let y = detail_content_area.y + detail_idx as u16;
            if y >= detail_content_area.y + detail_content_area.height {
                break;
            }
            let position = detail_idx + 1;
            let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
            let position_style = if is_active {
                Style::default().fg(p.text).bg(p.active_row_bg)
            } else {
                Style::default().fg(p.overlay0)
            };
            let (icon, icon_style) =
                state_icon(detail.state, detail.seen, app.status_indicators, p);

            if is_active {
                let buf = frame.buffer_mut();
                for x in detail_content_area.x..detail_content_area.x + detail_content_area.width {
                    buf[(x, y)].set_style(Style::default().bg(p.active_row_bg));
                }
            }

            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{position:<2}"), position_style),
                    Span::styled(icon, icon_style),
                ])),
                Rect::new(detail_content_area.x, y, detail_content_area.width, 1),
            );
        }
    }

    render_sidebar_toggle(app, frame, area, true, p);
}

pub(crate) fn workspace_drop_slots(
    app: &AppState,
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
) -> Vec<(crate::app::state::WorkspaceDropTarget, u16)> {
    if area.height == 0 || cards.is_empty() {
        return Vec::new();
    }
    let list_bottom = area.y + area.height.saturating_sub(1);
    let entries = workspace_list_entries(app);
    let entry_position = |ws_idx| {
        entries.iter().position(|entry| {
            matches!(
                entry,
                WorkspaceListEntry::Workspace {
                    ws_idx: entry_ws_idx,
                    ..
                } if *entry_ws_idx == ws_idx
            )
        })
    };
    let block_root_at = |entry_idx: usize| {
        entries[..=entry_idx]
            .iter()
            .rev()
            .find_map(|entry| match entry {
                WorkspaceListEntry::Workspace {
                    ws_idx,
                    indented: false,
                } => Some(*ws_idx),
                WorkspaceListEntry::Workspace { .. } => None,
            })
    };

    let mut slots = Vec::new();
    let mut previous_root = None;
    for card in cards.iter().filter(|card| !card.is_tag_header) {
        let Some(entry_idx) = entry_position(card.ws_idx) else {
            continue;
        };
        let Some(root_idx) = block_root_at(entry_idx) else {
            continue;
        };
        if previous_root == Some(root_idx) {
            continue;
        }
        previous_root = Some(root_idx);
        if let Some(row) = card.rect.y.checked_sub(1).filter(|row| *row < list_bottom) {
            slots.push((
                crate::app::state::WorkspaceDropTarget::Before(root_idx),
                row,
            ));
        }
    }

    let Some(last) = cards.iter().rev().find(|card| !card.is_tag_header) else {
        return slots;
    };
    let Some(last_entry_idx) = entry_position(last.ws_idx) else {
        return slots;
    };
    let next_entry = entries.get(last_entry_idx.saturating_add(1));
    if matches!(
        next_entry,
        Some(WorkspaceListEntry::Workspace { indented: true, .. })
    ) {
        return slots;
    }
    let target = match next_entry {
        Some(WorkspaceListEntry::Workspace { ws_idx, .. }) => {
            crate::app::state::WorkspaceDropTarget::Before(*ws_idx)
        }
        None => crate::app::state::WorkspaceDropTarget::End,
    };
    let row = last.rect.y.saturating_add(last.rect.height);
    if row < list_bottom
        && slots
            .last()
            .is_none_or(|(last_target, _)| *last_target != target)
    {
        slots.push((target, row));
    }
    slots
}

pub(crate) fn workspace_drop_indicator_row(
    app: &AppState,
    cards: &[crate::app::state::WorkspaceCardArea],
    area: Rect,
    target: crate::app::state::WorkspaceDropTarget,
) -> Option<u16> {
    workspace_drop_slots(app, cards, area)
        .into_iter()
        .find_map(|(candidate, row)| (candidate == target).then_some(row))
}

pub(super) fn render_sidebar(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;
    frame
        .buffer_mut()
        .set_style(area, Style::default().bg(p.sidebar_bg));
    let is_navigating = matches!(app.mode, Mode::Navigate);
    let sep_style = if is_navigating {
        Style::default().fg(p.accent)
    } else {
        Style::default().fg(p.surface_dim)
    };

    let sep_x = area.x + area.width.saturating_sub(1);
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        buf[(sep_x, y)].set_symbol("│");
        buf[(sep_x, y)].set_style(sep_style);
    }

    let (ws_area, detail_area) = expanded_sidebar_sections(area, app.sidebar_section_split);

    render_workspace_list(app, terminal_runtimes, frame, ws_area, is_navigating);
    render_agent_detail(app, terminal_runtimes, frame, detail_area);
    render_sidebar_toggle(app, frame, area, false, p);
}

fn resolved_token_spans(
    resolved: &[ResolvedToken],
    state_icon: (&str, Style),
    state_text_style: Style,
    workspace_style: Style,
    secondary_style: Style,
    custom_style: Style,
    p: &Palette,
    max_width: usize,
) -> Vec<Span<'static>> {
    let fixed_widths = resolved
        .iter()
        .map(|token| match &token.kind {
            ResolvedTokenKind::StateIcon => display_width(state_icon.0),
            ResolvedTokenKind::NeedEdge { .. } => 1,
            ResolvedTokenKind::GitStatus { ahead, behind } => {
                usize::from(*ahead > 0) * display_width(&format!("↑{ahead}"))
                    + usize::from(*behind > 0) * display_width(&format!("↓{behind}"))
                    + usize::from(*ahead > 0 && *behind > 0)
            }
            _ => 0,
        })
        .collect::<Vec<_>>();
    let flexible_widths = resolved
        .iter()
        .map(|token| match &token.kind {
            ResolvedTokenKind::StateText(text)
            | ResolvedTokenKind::Workspace(text)
            | ResolvedTokenKind::Tab(text)
            | ResolvedTokenKind::Pane(text)
            | ResolvedTokenKind::Agent(text)
            | ResolvedTokenKind::TerminalTitle(text)
            | ResolvedTokenKind::Branch(text)
            | ResolvedTokenKind::SpaceAgents(text)
            | ResolvedTokenKind::Waiting { text, .. }
            | ResolvedTokenKind::Custom(text) => display_width(text),
            _ => 0,
        })
        .collect::<Vec<_>>();
    let minimum_width = |active: &[bool]| {
        let indices = active
            .iter()
            .enumerate()
            .filter_map(|(index, active)| active.then_some(index))
            .collect::<Vec<_>>();
        let content = indices
            .iter()
            .map(|index| fixed_widths[*index] + usize::from(flexible_widths[*index] > 0))
            .sum::<usize>();
        let separators = indices
            .windows(2)
            .map(|pair| display_width(tokens::separator(&resolved[pair[0]], &resolved[pair[1]])))
            .sum::<usize>();
        content + separators
    };
    let mut active = resolved.iter().map(|_| true).collect::<Vec<_>>();
    if minimum_width(&active) > max_width {
        for (index, width) in flexible_widths.iter().enumerate() {
            if *width > 0 {
                active[index] = false;
            }
        }
        for index in (0..resolved.len()).rev() {
            if flexible_widths[index] == 0 {
                continue;
            }
            active[index] = true;
            if minimum_width(&active) > max_width {
                active[index] = false;
            }
        }
    }
    let visible_indices = active
        .iter()
        .enumerate()
        .filter_map(|(index, active)| active.then_some(index))
        .collect::<Vec<_>>();
    let separator_width = visible_indices
        .windows(2)
        .map(|pair| display_width(tokens::separator(&resolved[pair[0]], &resolved[pair[1]])))
        .sum::<usize>();
    let fixed_width = visible_indices
        .iter()
        .map(|index| fixed_widths[*index])
        .sum::<usize>();
    let mut budgets = flexible_widths
        .iter()
        .enumerate()
        .map(|(index, width)| usize::from(active[index] && *width > 0))
        .collect::<Vec<_>>();
    let minimum = budgets.iter().sum::<usize>();
    let mut remaining = max_width
        .saturating_sub(separator_width + fixed_width)
        .saturating_sub(minimum);
    while remaining > 0 {
        let mut grew = false;
        for (budget, width) in budgets.iter_mut().zip(&flexible_widths) {
            if *budget > 0 && *budget < *width {
                *budget += 1;
                remaining -= 1;
                grew = true;
                if remaining == 0 {
                    break;
                }
            }
        }
        if !grew {
            break;
        }
    }
    let mut spans = Vec::new();
    for (position, index) in visible_indices.iter().copied().enumerate() {
        let token = &resolved[index];
        if position > 0 {
            let previous = &resolved[visible_indices[position - 1]];
            spans.push(Span::styled(
                tokens::separator(previous, token),
                Style::default().fg(p.overlay0).add_modifier(Modifier::DIM),
            ));
        }
        match &token.kind {
            ResolvedTokenKind::StateIcon => {
                spans.push(Span::styled(
                    state_icon.0.to_string(),
                    apply_token_style(state_icon.1, token.style),
                ));
            }
            ResolvedTokenKind::StateText(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(state_text_style, token.style),
                ));
            }
            ResolvedTokenKind::Workspace(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(workspace_style, token.style),
                ));
            }
            ResolvedTokenKind::Tab(text)
            | ResolvedTokenKind::Pane(text)
            | ResolvedTokenKind::Agent(text)
            | ResolvedTokenKind::Branch(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(secondary_style, token.style),
                ));
            }
            ResolvedTokenKind::GitStatus { ahead, behind } => {
                if *ahead > 0 {
                    spans.push(Span::styled(
                        format!("↑{ahead}"),
                        apply_token_style(Style::default().fg(p.green), token.style),
                    ));
                }
                if *ahead > 0 && *behind > 0 {
                    spans.push(Span::styled(
                        " ",
                        apply_token_style(Style::default(), token.style),
                    ));
                }
                if *behind > 0 {
                    spans.push(Span::styled(
                        format!("↓{behind}"),
                        apply_token_style(Style::default().fg(p.red), token.style),
                    ));
                }
            }
            ResolvedTokenKind::NeedEdge { state, seen } => {
                let (symbol, color) = match (state, seen) {
                    (AgentState::Blocked, _) => ("▎", Some(p.red)),
                    (AgentState::Idle, false) => ("▎", Some(p.green)),
                    (AgentState::Idle, true) => ("▎", Some(p.yellow)),
                    (AgentState::Working | AgentState::Unknown, _) => (" ", None),
                };
                let style = color.map_or_else(Style::default, |color| Style::default().fg(color));
                spans.push(Span::styled(
                    symbol.to_string(),
                    apply_token_style(style, token.style),
                ));
            }
            ResolvedTokenKind::Waiting { text, state } => {
                let color = if *state == AgentState::Blocked {
                    p.red
                } else {
                    p.green
                };
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                        token.style,
                    ),
                ));
            }
            ResolvedTokenKind::TerminalTitle(text)
            | ResolvedTokenKind::SpaceAgents(text)
            | ResolvedTokenKind::Custom(text) => {
                spans.push(Span::styled(
                    truncate_end(text, budgets[index]),
                    apply_token_style(custom_style, token.style),
                ));
            }
        }
    }
    spans
}

fn apply_token_style(mut style: Style, patch: crate::config::SidebarTokenStyle) -> Style {
    if let Some(fg) = patch.fg {
        style = style.fg(fg.ratatui());
    }
    if let Some(bold) = patch.bold {
        style = if bold {
            style.add_modifier(Modifier::BOLD)
        } else {
            style.remove_modifier(Modifier::BOLD)
        };
    }
    if let Some(dim) = patch.dim {
        style = if dim {
            style.add_modifier(Modifier::DIM)
        } else {
            style.remove_modifier(Modifier::DIM)
        };
    }
    style
}

/// A theme-accent choice a tag can be painted with. `red` is deliberately absent:
/// red is reserved for broken/blocked state, so no tag ever borrows it. The `id()`
/// is the stable string persisted per tag name in the session snapshot; `label()`
/// is what the picker shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagAccent {
    Accent,
    Teal,
    Green,
    Yellow,
    Mauve,
}

impl TagAccent {
    /// The accents the picker offers, in menu order.
    pub(crate) const ALL: [TagAccent; 5] = [
        TagAccent::Accent,
        TagAccent::Teal,
        TagAccent::Green,
        TagAccent::Yellow,
        TagAccent::Mauve,
    ];

    /// Stable id persisted in the snapshot's `tag_colors` map.
    pub(crate) fn id(self) -> &'static str {
        match self {
            TagAccent::Accent => "accent",
            TagAccent::Teal => "teal",
            TagAccent::Green => "green",
            TagAccent::Yellow => "yellow",
            TagAccent::Mauve => "mauve",
        }
    }

    /// Human name shown beside the swatch in the picker.
    pub(crate) fn label(self) -> &'static str {
        match self {
            TagAccent::Accent => "Accent",
            TagAccent::Teal => "Teal",
            TagAccent::Green => "Green",
            TagAccent::Yellow => "Yellow",
            TagAccent::Mauve => "Mauve",
        }
    }

    /// Parse a persisted id back into an accent, ignoring unknown ids so a stale
    /// or hand-edited snapshot falls back to the stable-hash default.
    pub(crate) fn from_id(id: &str) -> Option<TagAccent> {
        TagAccent::ALL.into_iter().find(|accent| accent.id() == id)
    }

    /// Resolve the accent to a concrete color from the active palette.
    pub(crate) fn color(self, p: &Palette) -> Color {
        match self {
            TagAccent::Accent => p.accent,
            TagAccent::Teal => p.teal,
            TagAccent::Green => p.green,
            TagAccent::Yellow => p.yellow,
            TagAccent::Mauve => p.mauve,
        }
    }
}

/// Stable default accent for a tag, hashed from its bytes onto the picker's set.
/// Used when the tag has no explicit color override.
pub(crate) fn default_tag_accent(tag: &str) -> TagAccent {
    let hash = tag.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    });
    // Only the first four accents form the hash default, matching the historical
    // palette; `mauve` is reachable solely through an explicit pick.
    let hashed = [
        TagAccent::Accent,
        TagAccent::Teal,
        TagAccent::Green,
        TagAccent::Yellow,
    ];
    hashed[(hash as usize) % hashed.len()]
}

/// Color for a tag: the explicit per-tag-name override in `overrides` when one is
/// set (and parses to a known accent), otherwise the stable-hash default. `p.red`
/// is never returned. The same tag always maps to the same color absent an override.
pub(crate) fn tag_color_with_overrides(
    tag: &str,
    overrides: &std::collections::HashMap<String, String>,
    p: &Palette,
) -> Color {
    overrides
        .get(tag)
        .and_then(|id| TagAccent::from_id(id))
        .unwrap_or_else(|| default_tag_accent(tag))
        .color(p)
}

/// Stable color for a tag with no override map available; the hash default.
/// Only the tests need this now (production always routes through the override
/// map), so it is gated to test builds to stay clear of dead-code warnings.
#[cfg(test)]
pub(crate) fn tag_color(tag: &str, p: &Palette) -> Color {
    default_tag_accent(tag).color(p)
}

/// Leading attention edge for a tag group header: the strongest state across the
/// group's members, colored as the sidebar colors that state. `Working` shows no
/// colored edge (a dim space), matching the boards' quiet treatment of working.
fn tag_group_edge(need: Option<NeedLevel>, p: &Palette) -> Span<'static> {
    match need {
        Some(NeedLevel::Blocked) => Span::styled("▎", Style::default().fg(p.red)),
        Some(NeedLevel::NeedsYou) => Span::styled("▎", Style::default().fg(p.green)),
        Some(NeedLevel::Idle) => Span::styled("▎", Style::default().fg(p.yellow)),
        // Working or empty: no colored edge, just a placeholder cell so the
        // chevron column stays aligned across headers.
        _ => Span::styled(" ", Style::default().fg(p.overlay0)),
    }
}

fn render_tag_group_header(
    app: &AppState,
    frame: &mut Frame,
    card: &crate::app::state::WorkspaceCardArea,
    list_bottom: u16,
) {
    if card.rect.y >= list_bottom {
        return;
    }
    let p = &app.palette;
    let Some(tag) = app.workspaces.get(card.ws_idx).and_then(|ws| ws.tag()) else {
        return;
    };
    // Count top-level workspaces (worktree children fold into their parent block),
    // so the header count matches the number of rows the group expands to.
    let count = workspace_list_entries(app)
        .into_iter()
        .filter(|entry| {
            matches!(entry, WorkspaceListEntry::Workspace { indented: false, ws_idx }
                if app.workspaces.get(*ws_idx).and_then(|ws| ws.tag()) == Some(tag))
        })
        .count();
    // Rolled-up attention across every workspace carrying this tag drives the edge.
    let member_states: Vec<(AgentState, bool)> = app
        .workspaces
        .iter()
        .filter(|ws| ws.tag() == Some(tag))
        .map(|ws| ws.aggregate_state(&app.terminals))
        .collect();
    let need = need_rollup(&member_states);
    let color = tag_color_with_overrides(tag, &app.tag_colors, p);
    let collapsed = app.collapsed_space_keys.contains(&tag_collapse_key(tag));
    let chevron = if collapsed { "▸" } else { "▾" };
    let spans = vec![
        tag_group_edge(need, p),
        Span::styled(format!("{chevron} "), Style::default().fg(color)),
        Span::styled(
            tag.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {count}"), Style::default().fg(p.overlay0)),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(card.rect.x, card.rect.y, card.rect.width, 1),
    );
}

fn render_workspace_list(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
    is_navigating: bool,
) {
    let p = &app.palette;
    let dragged_ws_idx = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder { source_ws_idx, .. }) => {
            Some(*source_ws_idx)
        }
        _ => None,
    };
    let insertion_row = match app.drag.as_ref().map(|drag| &drag.target) {
        Some(crate::app::state::DragTarget::WorkspaceReorder {
            drop_target: Some(drop_target),
            ..
        }) => workspace_drop_indicator_row(app, &app.view.workspace_card_areas, area, *drop_target),
        _ => None,
    };

    let list_bottom = area.y + area.height.saturating_sub(1);
    if area.height > 0 {
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                " spaces",
                Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
            )])),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }

    let metrics = workspace_list_scroll_metrics(app, area);
    let scrollbar_rect = workspace_list_scrollbar_rect(app, area);
    let cards = &app.view.workspace_card_areas;
    let entries = workspace_list_entries(app);

    for card in cards {
        let i = card.ws_idx;
        if card.is_tag_header {
            render_tag_group_header(app, frame, card, list_bottom);
            continue;
        }
        let ws = &app.workspaces[i];
        let row_y = card.rect.y;
        let row_height = card.rect.height;
        let selected = i == app.selected && is_navigating;
        let is_active = Some(i) == app.active;
        let is_dragged = dragged_ws_idx == Some(i);
        let highlighted = selected || is_active || is_dragged;
        let (agg_state, agg_seen) = ws.aggregate_state(&app.terminals);

        if highlighted {
            let bg = if selected {
                workspace_selection_background(p, is_active)
            } else if is_dragged {
                p.surface1
            } else {
                p.active_row_bg
            };
            let buf = frame.buffer_mut();
            for y in row_y..row_y + row_height {
                if y >= list_bottom {
                    break;
                }
                for x in card.rect.x..card.rect.x + card.rect.width {
                    buf[(x, y)].set_style(Style::default().bg(bg));
                }
            }
        }

        let name_style = if selected || is_active || is_dragged {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.subtext0)
        };

        let label = ws.display_name_from(&app.terminals, terminal_runtimes);
        let display_label = if card.indented {
            grouped_child_display_label(&label, ws.branch().as_deref(), ws.custom_name.is_some())
        } else {
            label
        };
        let parent_group = (!card.indented)
            .then(|| workspace_parent_group_state(app, i))
            .flatten();
        let is_last_child = card.indented
            && entries
                .iter()
                .position(|entry| {
                    matches!(
                        entry,
                        WorkspaceListEntry::Workspace { ws_idx, .. } if *ws_idx == i
                    )
                })
                .is_none_or(|entry_idx| !next_entry_is_indented_workspace(&entries, entry_idx));
        let (display_state, display_seen) = parent_group
            .as_ref()
            .filter(|(_, collapsed)| *collapsed)
            .map(|(key, _)| space_aggregate_state(app, key))
            .unwrap_or((agg_state, agg_seen));
        let state_icon = state_icon(display_state, display_seen, app.status_indicators, p);
        let state_text_style = Style::default()
            .fg(state_label_color(display_state, display_seen, p))
            .add_modifier(Modifier::DIM);
        let branch_style = Style::default().fg(if selected || is_active {
            p.mauve
        } else {
            p.overlay0
        });
        let token_values = ws.metadata_tokens.values();
        let rows = tokens::space_rows(
            &app.sidebar_spaces,
            SpaceTokenContext {
                workspace: &display_label,
                branch: ws.branch().as_deref(),
                state_text: state_label(display_state, display_seen),
                ahead_behind: ws.git_ahead_behind(),
                tokens: &token_values,
                suppress_git_details: card.indented,
                agents: workspace_agent_summary(app, ws),
            },
        );

        // Leading attention edge for this row, colored by the workspace's own
        // rolled-up need exactly as the tag-group headers color theirs. Top-level
        // rows (grouped tag members included) carry it in their first cell, which
        // was previously blank pad, so nothing shifts. Worktree children keep their
        // tree-connector column untouched and take no edge.
        let row_edge = tag_group_edge(need_rollup(&[(agg_state, agg_seen)]), p);

        for (row_index, resolved) in rows.iter().enumerate() {
            if row_index as u16 >= row_height || row_y + row_index as u16 >= list_bottom {
                break;
            }
            let mut spans = Vec::new();
            let prefix_width = if card.indented {
                spans.push(Span::raw("   "));
                if row_index == 0 {
                    spans.push(Span::styled(
                        if is_last_child { "└─ " } else { "├─ " },
                        Style::default().fg(p.overlay0),
                    ));
                    6
                } else if is_last_child {
                    spans.push(Span::raw("     "));
                    8
                } else {
                    spans.push(Span::styled("│", Style::default().fg(p.overlay0)));
                    spans.push(Span::raw("    "));
                    8
                }
            } else if row_index == 0 {
                // Edge in the first cell, in place of the one-cell pad.
                spans.push(row_edge.clone());
                1
            } else {
                // Edge spans the row's later lines too, keeping the two-cell pad.
                spans.push(row_edge.clone());
                spans.push(Span::raw("  "));
                3
            };
            let trailing_width = if row_index == 0 && parent_group.is_some() {
                2
            } else {
                0
            };
            spans.extend(resolved_token_spans(
                resolved,
                state_icon,
                state_text_style,
                name_style,
                branch_style,
                branch_style,
                p,
                card.rect
                    .width
                    .saturating_sub(prefix_width + trailing_width) as usize,
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(card.rect.x, row_y + row_index as u16, card.rect.width, 1),
            );
        }

        if let Some((_, collapsed)) = parent_group {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    if collapsed { "▸" } else { "▾" },
                    Style::default().fg(p.accent),
                )),
                workspace_group_chevron_rect(card),
            );
        }
    }

    if let Some(y) = insertion_row.filter(|y| *y < list_bottom) {
        let indicator_right = scrollbar_rect
            .map(|rect| rect.x)
            .unwrap_or(area.x + area.width);
        let buf = frame.buffer_mut();
        for x in area.x..indicator_right {
            buf[(x, y)].set_symbol("─");
            buf[(x, y)].set_style(Style::default().fg(p.accent));
        }
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }

    if app.mouse_capture && list_bottom > area.y {
        let new_rect = app.sidebar_new_button_rect();
        frame.render_widget(
            Paragraph::new(Span::styled(" new", Style::default().fg(p.overlay0))),
            new_rect,
        );

        let menu_rect = app.global_launcher_rect();
        let menu_line = if app.global_menu_attention_badge_visible() {
            Line::from(vec![
                Span::styled(
                    "● ",
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled("menu", Style::default().fg(p.overlay0)),
            ])
        } else {
            Line::from(vec![Span::styled("menu", Style::default().fg(p.overlay0))])
        };
        frame.render_widget(
            Paragraph::new(menu_line).alignment(Alignment::Right),
            menu_rect,
        );
    }
}

/// Render one agents-panel tag group header at `row_y`: leading attention edge,
/// colored chevron, bold tag name in its stable color, dim count. Mirrors the
/// spaces list's `render_tag_group_header`; the agents panel does not indent
/// member rows, so the header carries no indent either.
fn render_agent_tag_group_header(
    app: &AppState,
    frame: &mut Frame,
    body: Rect,
    row_y: u16,
    tag: &str,
    count: usize,
    collapsed: bool,
    need: Option<NeedLevel>,
) {
    let p = &app.palette;
    let color = tag_color_with_overrides(tag, &app.tag_colors, p);
    let chevron = if collapsed { "▸" } else { "▾" };
    let spans = vec![
        tag_group_edge(need, p),
        Span::styled(format!("{chevron} "), Style::default().fg(color)),
        Span::styled(
            tag.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {count}"), Style::default().fg(p.overlay0)),
    ];
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(body.x, row_y, body.width, 1),
    );
}

fn render_agent_detail(
    app: &AppState,
    terminal_runtimes: &TerminalRuntimeRegistry,
    frame: &mut Frame,
    area: Rect,
) {
    let p = &app.palette;

    if area.height < 3 {
        return;
    }

    let sep_line = "─".repeat(area.width as usize);
    frame.render_widget(
        Paragraph::new(Span::styled(&sep_line, Style::default().fg(p.surface_dim))),
        Rect::new(area.x, area.y, area.width, 1),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            " agents",
            Style::default().fg(p.overlay0).add_modifier(Modifier::BOLD),
        )])),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
    let control_label = active_agent_view_label(app)
        .unwrap_or_else(|| agent_panel_sort_label(app.agent_panel_sort));
    let toggle_rect = agent_panel_header_label_rect(area, control_label);
    if toggle_rect != Rect::default() {
        let color = if app.agent_view_override.is_some() {
            p.accent
        } else {
            p.overlay0
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                control_label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            toggle_rect,
        );
    }

    let details = agent_panel_entries_from(app, terminal_runtimes);
    let metrics = agent_panel_scroll_metrics(app, area);
    let scrollbar_rect = agent_panel_scrollbar_rect(app, area);
    let body = agent_panel_body_rect(area, should_show_scrollbar(metrics));
    if body == Rect::default() {
        return;
    }
    if details.is_empty() && app.agent_view_override.is_some() {
        frame.render_widget(
            Paragraph::new(" no matching agents")
                .style(Style::default().fg(p.overlay0).add_modifier(Modifier::DIM)),
            Rect::new(body.x, body.y, body.width, 1),
        );
        return;
    }

    let scroll = app.agent_panel_scroll.min(metrics.max_offset_from_bottom);
    let mut row_y = body.y;
    let body_bottom = body.y + body.height;
    let display_rows = agent_panel_display_rows(app, &details);
    for (index, row) in display_rows.iter().enumerate().skip(scroll) {
        let gap = agent_display_row_gap(app, &display_rows, index);
        let entry_idx = match row {
            AgentPanelRow::Header {
                tag,
                count,
                collapsed,
                need,
            } => {
                if row_y >= body_bottom {
                    break;
                }
                render_agent_tag_group_header(
                    app, frame, body, row_y, tag, *count, *collapsed, *need,
                );
                row_y = row_y.saturating_add(1).saturating_add(gap).min(body_bottom);
                continue;
            }
            AgentPanelRow::Entry { entry_idx } => *entry_idx,
        };
        let Some(detail) = details.get(entry_idx) else {
            continue;
        };
        let label_color = state_label_color(detail.state, detail.seen, p);
        let rows = resolved_agent_rows(app, detail);
        let height = (rows.len().max(1) as u16).min(body.height);
        if row_y.saturating_add(height) > body_bottom {
            break;
        }

        let is_active = app.is_active_pane(detail.ws_idx, detail.tab_idx, detail.pane_id);
        let row_style = if is_active {
            Style::default().bg(p.active_row_bg)
        } else {
            Style::default()
        };
        let name_style = if is_active {
            Style::default().fg(p.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.subtext0).add_modifier(Modifier::BOLD)
        };
        let status_style = if is_active {
            Style::default().fg(label_color)
        } else {
            Style::default().fg(label_color).add_modifier(Modifier::DIM)
        };
        let agent_style = Style::default().fg(p.overlay0).add_modifier(Modifier::DIM);
        let state_icon = state_icon(detail.state, detail.seen, app.status_indicators, p);

        for (row_index, resolved) in rows.iter().take(height as usize).enumerate() {
            let mut spans = vec![Span::raw(if row_index == 0 { " " } else { "   " })];
            spans.extend(resolved_token_spans(
                resolved,
                state_icon,
                status_style,
                name_style,
                agent_style,
                agent_style,
                p,
                body.width
                    .saturating_sub(if row_index == 0 { 1 } else { 3 }) as usize,
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(row_style),
                Rect::new(body.x, row_y + row_index as u16, body.width, 1),
            );
        }
        row_y = row_y
            .saturating_add(height)
            .saturating_add(gap)
            .min(body_bottom);
    }

    if let Some(track) = scrollbar_rect {
        render_scrollbar(frame, metrics, track, p.surface_dim, p.overlay0, "▕");
    }
}

pub(crate) fn collapsed_sidebar_toggle_rect(area: Rect) -> Rect {
    let bottom_y = area.y + area.height.saturating_sub(1);
    let content_w = area.width.saturating_sub(1);
    if content_w == 0 || area.height == 0 {
        return Rect::default();
    }
    let x = area.x + content_w / 2;
    Rect::new(x, bottom_y, 1, 1)
}

pub(crate) fn expanded_sidebar_toggle_rect(area: Rect) -> Rect {
    if area.width <= 1 || area.height == 0 {
        return Rect::default();
    }
    Rect::new(
        area.x + area.width.saturating_sub(2),
        area.y + area.height.saturating_sub(1),
        1,
        1,
    )
}

fn render_sidebar_toggle(
    app: &AppState,
    frame: &mut Frame,
    area: Rect,
    collapsed: bool,
    p: &Palette,
) {
    let toggle_area = if collapsed {
        collapsed_sidebar_toggle_rect(area)
    } else {
        expanded_sidebar_toggle_rect(area)
    };
    if toggle_area == Rect::default() {
        return;
    }
    let icon = if collapsed { "»" } else { "«" };
    let icon_style = if collapsed && app.global_menu_attention_badge_visible() {
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(p.overlay0)
    };
    frame.render_widget(Paragraph::new(Span::styled(icon, icon_style)), toggle_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{detect::Agent, layout::PaneId, workspace::Workspace};
    use ratatui::{backend::TestBackend, layout::Direction, Terminal};

    fn row_text(buffer: &ratatui::buffer::Buffer, row: u16, width: u16) -> String {
        (0..width)
            .map(|x| buffer[(x, row)].symbol())
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn find_symbol_x(buffer: &ratatui::buffer::Buffer, row: u16, width: u16, symbol: &str) -> u16 {
        (0..width)
            .find(|x| buffer[(*x, row)].symbol() == symbol)
            .unwrap_or_else(|| {
                panic!(
                    "missing symbol {symbol:?} in row {}",
                    row_text(buffer, row, width)
                )
            })
    }

    #[test]
    fn expanded_and_collapsed_sidebars_use_custom_background() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces.clear();
        app.active = None;
        app.palette.sidebar_bg = ratatui::style::Color::Rgb(12, 34, 56);
        let area = Rect::new(0, 0, 26, 20);

        let mut expanded = Terminal::new(TestBackend::new(26, 20)).unwrap();
        expanded
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        assert!(expanded
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.bg == app.palette.sidebar_bg));

        let mut collapsed = Terminal::new(TestBackend::new(26, 20)).unwrap();
        collapsed
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .unwrap();
        assert!(collapsed
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| cell.bg == app.palette.sidebar_bg));
    }

    #[test]
    fn default_agent_rows_remove_redundant_state_text() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal_state = app.terminals.get_mut(&terminal_id).unwrap();
        terminal_state.detected_agent = Some(Agent::Pi);
        terminal_state.state = AgentState::Working;

        let area = Rect::new(0, 0, 26, 20);
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false);

        let first = row_text(buffer, body.y, 25);
        let second = row_text(buffer, body.y + 1, 25);
        assert!(first.contains("one"));
        assert_eq!(second, "   pi");
        assert!(!first.contains("working"));
        assert!(!second.contains("working"));

        let workspace_x = find_symbol_x(buffer, body.y, body.width, "o");
        let workspace_style = buffer[(workspace_x, body.y)].style();
        assert_eq!(workspace_style.fg, Some(app.palette.text));
        assert!(workspace_style.add_modifier.contains(Modifier::BOLD));
        assert!(!workspace_style.add_modifier.contains(Modifier::DIM));
        assert_eq!(workspace_style.bg, Some(app.palette.active_row_bg));

        let agent_x = find_symbol_x(buffer, body.y + 1, body.width, "p");
        let agent_style = buffer[(agent_x, body.y + 1)].style();
        assert_eq!(agent_style.fg, Some(app.palette.overlay0));
        assert!(agent_style.add_modifier.contains(Modifier::DIM));
        assert!(!agent_style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(agent_style.bg, Some(app.palette.active_row_bg));
    }

    #[test]
    fn occurrence_false_removes_default_workspace_bold_and_agent_dim() {
        let config: crate::config::Config = toml::from_str(
            r##"
[ui.sidebar.agents]
rows = [[{ token = "workspace", bold = false }, { token = "agent", dim = false }]]
"##,
        )
        .unwrap();
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_agents = config.ui.sidebar.agents;
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        app.active = Some(0);
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);

        let area = Rect::new(0, 0, 26, 20);
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false);
        let buffer = terminal.backend().buffer();
        let workspace = buffer[(find_symbol_x(buffer, body.y, body.width, "o"), body.y)].style();
        let agent = buffer[(find_symbol_x(buffer, body.y, body.width, "p"), body.y)].style();

        assert_eq!(workspace.fg, Some(app.palette.text));
        assert!(!workspace.add_modifier.contains(Modifier::BOLD));
        assert_eq!(agent.fg, Some(app.palette.overlay0));
        assert!(!agent.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn default_space_workspace_style_tracks_active_state() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.mode = Mode::Terminal;
        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let first_row = app.view.workspace_card_areas[0].rect.y;
        let second_row = app.view.workspace_card_areas[1].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let active = buffer[(find_symbol_x(buffer, first_row, 25, "o"), first_row)].style();
        assert_eq!(active.fg, Some(app.palette.text));
        assert!(active.add_modifier.contains(Modifier::BOLD));
        assert!(!active.add_modifier.contains(Modifier::DIM));
        assert_eq!(active.bg, Some(app.palette.active_row_bg));

        let inactive = buffer[(find_symbol_x(buffer, second_row, 25, "t"), second_row)].style();
        assert_eq!(inactive.fg, Some(app.palette.subtext0));
        assert!(!inactive
            .add_modifier
            .intersects(Modifier::BOLD | Modifier::DIM));
        assert_eq!(inactive.bg, Some(ratatui::style::Color::Reset));
    }

    #[test]
    fn navigate_selection_keeps_its_existing_background_beside_active_workspace() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.selected = 1;
        app.mode = Mode::Navigate;
        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let active_row = app.view.workspace_card_areas[0].rect.y;
        let selected_row = app.view.workspace_card_areas[1].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(
            buffer[(0, active_row)].bg,
            app.palette.active_row_bg,
            "active workspace should keep its dedicated background"
        );
        assert_eq!(
            buffer[(0, selected_row)].bg,
            app.palette.selection_bg,
            "navigate selection should use its dedicated cursor background"
        );
    }

    #[test]
    fn selected_active_workspace_resolves_expanded_background() {
        let mut app = crate::app::state::AppState::test_new();
        app.palette = crate::app::state::Palette::terminal();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Navigate;
        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let active_row = app.view.workspace_card_areas[0].rect.y;
        let inactive_row = app.view.workspace_card_areas[1].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();

        assert_eq!(
            terminal.backend().buffer()[(0, active_row)].bg,
            app.palette.active_row_bg
        );

        app.selected = 1;
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(0, active_row)].bg,
            app.palette.active_row_bg
        );
        assert_eq!(
            terminal.backend().buffer()[(0, inactive_row)].bg,
            app.palette.selection_bg
        );

        app.palette = crate::app::state::Palette::catppuccin();
        app.selected = 0;
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(0, active_row)].bg,
            app.palette.selection_bg
        );
    }

    #[test]
    fn selected_active_workspace_resolves_collapsed_background() {
        let mut app = crate::app::state::AppState::test_new();
        app.palette = crate::app::state::Palette::terminal();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Navigate;
        let area = Rect::new(0, 0, 5, 8);
        let mut terminal = Terminal::new(TestBackend::new(5, 8)).unwrap();
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .unwrap();

        let (workspace_area, _, _) = collapsed_sidebar_sections(area);
        assert_eq!(
            terminal.backend().buffer()[(workspace_area.x, workspace_area.y)].bg,
            app.palette.active_row_bg
        );

        app.selected = 1;
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(workspace_area.x, workspace_area.y)].bg,
            app.palette.active_row_bg
        );
        assert_eq!(
            terminal.backend().buffer()[(workspace_area.x, workspace_area.y + 1)].bg,
            app.palette.selection_bg
        );

        app.palette = crate::app::state::Palette::catppuccin();
        app.selected = 0;
        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(workspace_area.x, workspace_area.y)].bg,
            app.palette.selection_bg
        );
    }

    #[test]
    fn space_occurrence_style_applies_without_styling_separator() {
        let config: crate::config::Config = toml::from_str(
            r##"
[ui.sidebar.spaces]
rows = [[{ token = "$hype", fg = "#abcdef", bold = true, dim = false }, "workspace"]]
"##,
        )
        .unwrap();
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_spaces = config.ui.sidebar.spaces;
        app.workspaces = vec![Workspace::test_new("one")];
        app.active = Some(0);
        app.mode = Mode::Terminal;
        app.workspaces[0].metadata_tokens.patch(
            std::collections::HashMap::from([("hype".into(), Some("HI".into()))]),
            None,
            std::time::Instant::now(),
        );

        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let row = app.view.workspace_card_areas[0].rect.y;
        let mut terminal = Terminal::new(TestBackend::new(26, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let h = buffer[(find_symbol_x(buffer, row, 25, "H"), row)].style();
        let i = buffer[(find_symbol_x(buffer, row, 25, "I"), row)].style();
        let separator = buffer[(find_symbol_x(buffer, row, 25, "·"), row)].style();

        for style in [h, i] {
            assert_eq!(style.fg, Some(ratatui::style::Color::Rgb(0xab, 0xcd, 0xef)));
            assert!(style.add_modifier.contains(Modifier::BOLD));
            assert!(!style.add_modifier.contains(Modifier::DIM));
            assert_eq!(style.bg, Some(app.palette.active_row_bg));
        }
        assert_eq!(separator.fg, Some(app.palette.overlay0));
        assert!(separator.add_modifier.contains(Modifier::DIM));
        assert!(!separator.add_modifier.contains(Modifier::BOLD));
        assert_eq!(separator.bg, Some(app.palette.active_row_bg));
    }

    #[test]
    fn occurrence_foreground_flattens_composite_git_status_colors() {
        let config: crate::config::Config = toml::from_str(
            r##"[ui.sidebar.spaces]
rows = [[{ token = "git_status", fg = "#123456" }]]
"##,
        )
        .unwrap();
        let spans = resolved_token_spans(
            &[ResolvedToken {
                kind: ResolvedTokenKind::GitStatus {
                    ahead: 2,
                    behind: 1,
                },
                style: config.ui.sidebar.spaces.rows[0][0].parts().1,
            }],
            ("", Style::default()),
            Style::default(),
            Style::default(),
            Style::default(),
            Style::default(),
            &crate::app::state::AppState::test_new().palette,
            20,
        );

        assert_eq!(
            spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "↑2 ↓1"
        );
        assert!(spans
            .iter()
            .all(|span| { span.style.fg == Some(ratatui::style::Color::Rgb(0x12, 0x34, 0x56)) }));
    }

    #[test]
    fn default_agent_row_gap_packs_rendering_and_scroll_geometry() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.ensure_test_terminals();
        for (workspace, agent) in app.workspaces.iter().zip([Agent::Pi, Agent::Claude]) {
            let pane_id = workspace.tabs[0].root_pane;
            let terminal_id = workspace.tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(agent);
        }
        app.sidebar_agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
        assert_eq!(app.sidebar_agents.row_gap, 0);

        let area = Rect::new(0, 0, 20, 5);
        let metrics = agent_panel_scroll_metrics(&app, area);
        let body = agent_panel_body_rect(area, false);
        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        terminal
            .draw(|frame| render_agent_detail(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        assert_eq!(metrics.viewport_rows, 2);
        assert_eq!(metrics.max_offset_from_bottom, 0);
        assert_eq!(row_text(buffer, body.y, body.width), " pi");
        assert_eq!(row_text(buffer, body.y + 1, body.width), " claude");
    }

    #[test]
    fn narrow_agent_rows_preserve_later_tab_tokens() {
        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("very-long-workspace-name");
        let tab_idx = workspace.test_add_tab(Some("logs"));
        let pane_id = workspace.tabs[tab_idx].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[tab_idx].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);

        let area = Rect::new(0, 0, 18, 20);
        let mut terminal = Terminal::new(TestBackend::new(18, 20)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false);
        let first = row_text(buffer, body.y, 17);

        assert!(first.contains("logs"), "rendered row: {first:?}");
        assert!(first.contains('·'), "rendered row: {first:?}");
    }

    #[test]
    fn stripped_terminal_title_renders_with_unicode_width_truncation() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.detected_agent = Some(Agent::Claude);
        terminal.set_terminal_title(Some("⠋ 修复🙂标题很长".into()));
        app.sidebar_agents.rows = vec![vec![
            crate::config::AgentSidebarToken::TerminalTitleStripped,
        ]];

        let area = Rect::new(0, 0, 10, 12);
        let mut renderer = Terminal::new(TestBackend::new(10, 12)).unwrap();
        renderer
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false);
        let rendered = row_text(renderer.backend().buffer(), body.y, 9);

        assert!(!rendered.contains('⠋'));
        assert!(rendered.contains('修') && rendered.contains('复'));

        let spans = resolved_token_spans(
            &[ResolvedToken::unstyled(ResolvedTokenKind::TerminalTitle(
                "修复🙂标题很长".into(),
            ))],
            ("", Style::default()),
            Style::default(),
            Style::default(),
            Style::default(),
            Style::default(),
            &app.palette,
            8,
        );
        let text = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(display_width(&text) <= 8, "resolved title: {text:?}");
    }

    #[test]
    fn variable_agent_heights_pack_the_bottom_and_reveal_targets() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("one"),
            Workspace::test_new("two"),
            Workspace::test_new("three"),
        ];
        app.ensure_test_terminals();
        for workspace in &app.workspaces {
            let pane_id = workspace.tabs[0].root_pane;
            let terminal_id = workspace.tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Pi);
        }
        let first_pane = app.workspaces[0].tabs[0].root_pane;
        let first_terminal = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&first_terminal)
            .unwrap()
            .metadata_tokens
            .patch(
                std::collections::HashMap::from([
                    ("a".into(), Some("a".into())),
                    ("b".into(), Some("b".into())),
                ]),
                None,
                std::time::Instant::now(),
            );
        app.sidebar_agents.rows = vec![
            vec![crate::config::AgentSidebarToken::Agent],
            vec![crate::config::AgentSidebarToken::Custom("a".into())],
            vec![crate::config::AgentSidebarToken::Custom("b".into())],
        ];
        let area = Rect::new(0, 0, 20, 6);

        let metrics = agent_panel_scroll_metrics(&app, area);
        assert_eq!(metrics.max_offset_from_bottom, 1);
        assert_eq!(agent_panel_scroll_for_target(&app, area, 0, 2), 1);
    }

    #[test]
    fn oversized_space_layout_is_clipped_to_the_section_body() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]; 6];
        let area = Rect::new(0, 0, 20, 10);
        let workspace_area = workspace_list_rect(area, app.sidebar_section_split);
        let body = workspace_list_body_rect(workspace_area, false);

        let metrics = workspace_list_scroll_metrics(&app, workspace_area);
        let (cards, _) = compute_workspace_list_areas(&app, area);

        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].ws_idx, 0);
        assert_eq!(cards[0].rect.height, body.height);
    }

    #[test]
    fn oversized_agent_override_is_clipped_to_the_panel_body() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("one");
        let pane_id = workspace.tabs[0].root_pane;
        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane_id]
            .attached_terminal_id
            .clone();
        app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        app.sidebar_agents.rows_by_agent.insert(
            "claude".into(),
            vec![vec![crate::config::AgentSidebarToken::Agent]; 6],
        );
        let panel = Rect::new(0, 0, 20, 5);

        let metrics = agent_panel_scroll_metrics(&app, panel);

        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(metrics.max_offset_from_bottom, 0);
        let entry = agent_panel_entries(&app).pop().unwrap();
        assert_eq!(
            agent_entry_height_in_body(&app, &entry, agent_panel_body_rect(panel, false).height),
            agent_panel_body_rect(panel, false).height
        );
    }

    #[test]
    fn render_sidebar_toggle_draws_expanded_collapse_icon() {
        let app = crate::app::state::AppState::test_new();
        let area = Rect::new(0, 0, 26, 20);
        let mut terminal =
            Terminal::new(TestBackend::new(26, 20)).expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_toggle(&app, frame, area, false, &app.palette))
            .expect("sidebar toggle should render");

        let toggle = expanded_sidebar_toggle_rect(area);
        assert_eq!(
            terminal.backend().buffer()[(toggle.x, toggle.y)].symbol(),
            "«"
        );
    }

    #[test]
    fn expanded_sidebar_toggle_sits_inside_sidebar_content() {
        let area = Rect::new(0, 0, 26, 20);
        let toggle = expanded_sidebar_toggle_rect(area);

        assert_eq!(toggle.x, area.x + area.width - 2);
        assert_eq!(toggle.y, area.y + area.height - 1);
    }

    #[test]
    fn agent_panel_tab_label_visibility_tracks_tab_identity() {
        let mut app = crate::app::state::AppState::test_new();
        let single_auto = Workspace::test_new("auto");
        let mut single_custom = Workspace::test_new("custom");
        single_custom.tabs[0].set_custom_name("focus".into());
        let mut multi = Workspace::test_new("multi");
        multi.test_add_tab(Some("logs"));

        app.workspaces = vec![single_auto, single_custom, multi];
        app.ensure_test_terminals();
        for (ws_idx, tab_idx, agent) in [
            (0, 0, Agent::Pi),
            (1, 0, Agent::Claude),
            (2, 0, Agent::Codex),
            (2, 1, Agent::Pi),
        ] {
            let pane_id = app.workspaces[ws_idx].tabs[tab_idx].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[tab_idx].panes[&pane_id]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(agent);
        }

        let entries = agent_panel_entries(&app);
        let labels: Vec<_> = entries
            .iter()
            .map(|entry| {
                (
                    entry.primary_label.as_str(),
                    entry.primary_tab_label.as_deref(),
                )
            })
            .collect();

        assert_eq!(
            labels,
            [
                ("auto", None),
                ("custom", Some("focus")),
                ("multi", Some("1")),
                ("multi", Some("logs")),
            ]
        );
    }

    #[test]
    fn priority_agent_panel_sort_uses_attention_then_space_order() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("one"),
            Workspace::test_new("two"),
            Workspace::test_new("three"),
            Workspace::test_new("four"),
        ];
        app.ensure_test_terminals();
        app.active = Some(0);
        app.selected = 0;
        app.agent_panel_sort = crate::app::state::AgentPanelSort::Priority;

        let set_state = |app: &mut crate::app::state::AppState, ws_idx: usize, state| {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            let terminal = app.terminals.get_mut(&terminal_id).unwrap();
            terminal.detected_agent = Some(Agent::Claude);
            terminal.state = state;
        };
        set_state(&mut app, 0, AgentState::Working);
        set_state(&mut app, 1, AgentState::Idle);
        set_state(&mut app, 2, AgentState::Working);
        set_state(&mut app, 3, AgentState::Blocked);

        let done_pane = app.workspaces[1].tabs[0].root_pane;
        app.workspaces[1].tabs[0]
            .panes
            .get_mut(&done_pane)
            .unwrap()
            .seen = false;

        let labels: Vec<String> = agent_panel_entries(&app)
            .into_iter()
            .map(|entry| entry.primary_label)
            .collect();

        assert_eq!(labels, ["four", "two", "one", "three"]);
    }

    #[test]
    fn collapsed_sidebar_numbers_grouped_agents_by_list_position() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![Workspace::test_new("one"), Workspace::test_new("two")];
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 12);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(detail_area.x, detail_area.y)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 1)].symbol(), "2");
    }

    /// Two agent panes in one workspace plus a second workspace, so the
    /// assertions can tell pane-level highlighting apart from workspace-level.
    fn collapsed_agent_app() -> (crate::app::state::AppState, PaneId, PaneId) {
        let mut app = crate::app::state::AppState::test_new();
        let mut first = Workspace::test_new("one");
        let second_pane = first.test_split(Direction::Horizontal);
        let first_pane = first.tabs[0].root_pane;
        app.workspaces = vec![first, Workspace::test_new("two")];
        app.ensure_test_terminals();

        let terminal_ids: Vec<_> = app
            .workspaces
            .iter()
            .flat_map(|ws| ws.tabs.iter())
            .flat_map(|tab| tab.panes.values())
            .map(|pane| pane.attached_terminal_id.clone())
            .collect();
        for terminal_id in terminal_ids {
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        (app, first_pane, second_pane)
    }

    fn collapsed_agent_row_styles(
        app: &crate::app::state::AppState,
        area: Rect,
        detail_area: Rect,
        rows: u16,
    ) -> Vec<Vec<ratatui::style::Style>> {
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");
        terminal
            .draw(|frame| render_sidebar_collapsed(app, frame, area))
            .expect("collapsed sidebar should render");
        let buffer = terminal.backend().buffer();
        (0..rows)
            .map(|row| {
                (detail_area.x..detail_area.x + detail_area.width)
                    .map(|x| buffer[(x, detail_area.y + row)].style())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn collapsed_sidebar_highlights_only_the_focused_agent_pane() {
        let (mut app, first_pane, second_pane) = collapsed_agent_app();
        app.active = Some(0);
        app.workspaces[0].tabs[0].layout.focus_pane(second_pane);
        assert!(app.is_active_pane(0, 0, second_pane));
        assert!(!app.is_active_pane(0, 0, first_pane));

        let area = Rect::new(0, 0, 4, 14);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let rows = collapsed_agent_row_styles(&app, area, detail_area, 3);

        let highlighted: Vec<_> = rows
            .iter()
            .filter(|cells| {
                cells
                    .iter()
                    .all(|style| style.bg == Some(app.palette.active_row_bg))
            })
            .collect();
        assert_eq!(
            highlighted.len(),
            1,
            "only the focused agent pane should be highlighted, across the whole row"
        );
        assert_eq!(highlighted[0][0].fg, Some(app.palette.text));

        let muted = rows
            .iter()
            .filter(|cells| cells[0].fg == Some(app.palette.overlay0))
            .count();
        assert_eq!(
            muted, 2,
            "the sibling pane in the active workspace and the other workspace stay muted"
        );
    }

    #[test]
    fn collapsed_sidebar_does_not_highlight_agents_without_active_workspace() {
        let (mut app, _, _) = collapsed_agent_app();
        app.active = None;

        let area = Rect::new(0, 0, 4, 14);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let rows = collapsed_agent_row_styles(&app, area, detail_area, 3);

        for cells in rows {
            assert_eq!(cells[0].fg, Some(app.palette.overlay0));
            for style in cells {
                assert_ne!(style.bg, Some(app.palette.active_row_bg));
            }
        }
    }

    #[test]
    fn collapsed_sidebar_keeps_workspace_status_visible_for_two_digit_positions() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = (1..=10)
            .map(|idx| Workspace::test_new(&format!("workspace-{idx}")))
            .collect();
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 25);
        let (workspace_area, _, _) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let tenth_row = workspace_area.y + 9;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(workspace_area.x, workspace_area.y)].symbol(), "1");
        assert_eq!(
            buffer[(workspace_area.x + 1, workspace_area.y)].symbol(),
            " "
        );
        assert_eq!(
            buffer[(workspace_area.x + 2, workspace_area.y)].symbol(),
            "·"
        );
        assert_eq!(buffer[(workspace_area.x, tenth_row)].symbol(), "1");
        assert_eq!(buffer[(workspace_area.x + 1, tenth_row)].symbol(), "0");
        assert_eq!(buffer[(workspace_area.x + 2, tenth_row)].symbol(), "·");
    }

    #[test]
    fn collapsed_sidebar_keeps_status_visible_for_two_digit_positions() {
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = (1..=10)
            .map(|idx| Workspace::test_new(&format!("workspace-{idx}")))
            .collect();
        app.ensure_test_terminals();

        for ws_idx in 0..app.workspaces.len() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().detected_agent = Some(Agent::Claude);
        }

        let area = Rect::new(0, 0, 4, 25);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let tenth_row = detail_area.y + 9;
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(detail_area.x, tenth_row)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x + 1, tenth_row)].symbol(), "0");
        assert_eq!(buffer[(detail_area.x + 2, tenth_row)].symbol(), "·");
    }

    #[test]
    fn collapsed_sidebar_numbers_priority_agents_by_list_position() {
        let first = Workspace::test_new("one");
        let first_pane = first.tabs[0].root_pane;
        let mut second = Workspace::test_new("two");
        let second_pane = second.tabs[0].root_pane;
        let urgent_pane = second.test_split(ratatui::layout::Direction::Horizontal);

        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![first, second];
        app.ensure_test_terminals();
        app.agent_panel_sort = crate::app::state::AgentPanelSort::Priority;
        app.status_indicators = crate::config::StatusIndicatorStyle::Symbols;

        let set_state = |app: &mut crate::app::state::AppState, ws_idx: usize, pane_id, state| {
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane_id]
                .attached_terminal_id
                .clone();
            let terminal = app.terminals.get_mut(&terminal_id).unwrap();
            terminal.detected_agent = Some(Agent::Claude);
            terminal.state = state;
        };
        set_state(&mut app, 0, first_pane, AgentState::Idle);
        set_state(&mut app, 1, second_pane, AgentState::Working);
        set_state(&mut app, 1, urgent_pane, AgentState::Blocked);
        app.workspaces[0].tabs[0]
            .panes
            .get_mut(&first_pane)
            .unwrap()
            .seen = false;

        assert_eq!(app.workspaces[1].public_pane_number(urgent_pane), Some(2));
        assert_eq!(agent_panel_entries(&app)[0].pane_id, urgent_pane);

        let area = Rect::new(0, 0, 4, 16);
        let (_, _, detail_area) = collapsed_sidebar_sections(area);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height))
            .expect("test terminal should initialize");

        terminal
            .draw(|frame| render_sidebar_collapsed(&app, frame, area))
            .expect("collapsed sidebar should render");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(detail_area.x, detail_area.y)].symbol(), "1");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 1)].symbol(), "2");
        assert_eq!(buffer[(detail_area.x, detail_area.y + 2)].symbol(), "3");
        assert_eq!(buffer[(detail_area.x + 2, detail_area.y)].symbol(), "×");
        assert_eq!(
            buffer[(detail_area.x + 2, detail_area.y)].style().fg,
            Some(app.palette.red)
        );
        assert_eq!(buffer[(detail_area.x + 2, detail_area.y + 1)].symbol(), "✓");
        assert_eq!(
            buffer[(detail_area.x + 2, detail_area.y + 1)].style().fg,
            Some(app.palette.teal)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn all_workspaces_agent_panel_entries_use_live_root_runtime_cwd_for_workspace_label() {
        let unique = format!(
            "herdr-agent-panel-runtime-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let stale_cwd = root.join("issue-264-nix-support");
        let live_cwd = root.join("herdr");
        std::fs::create_dir_all(stale_cwd.join(".git")).unwrap();
        std::fs::create_dir_all(live_cwd.join(".git")).unwrap();

        let mut app = crate::app::state::AppState::test_new();
        let mut workspace = Workspace::test_new("stale-name");
        workspace.custom_name = None;
        workspace.identity_cwd = stale_cwd.clone();
        let pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let terminal_id = app.workspaces[0].tabs[0].panes[&pane]
            .attached_terminal_id
            .clone();
        let terminal = app.terminals.get_mut(&terminal_id).unwrap();
        terminal.cwd = stale_cwd;
        terminal.detected_agent = Some(Agent::Pi);
        app.active = Some(0);
        app.selected = 0;

        let (events, _) = tokio::sync::mpsc::channel(4);
        let runtime = crate::terminal::TerminalRuntime::spawn(
            pane,
            24,
            80,
            live_cwd.clone(),
            0,
            crate::terminal_theme::TerminalTheme::default(),
            None,
            crate::pane::PaneShellConfig::new("/bin/sh", crate::config::ShellModeConfig::NonLogin),
            &crate::pane::PaneLaunchEnv::default(),
            events,
            std::sync::Arc::new(tokio::sync::Notify::new()),
            std::sync::Arc::new(crate::render_signal::RenderSignal::new()),
        )
        .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while runtime.cwd() != Some(live_cwd.clone()) && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let mut runtime_registry = TerminalRuntimeRegistry::new();
        runtime_registry.insert(terminal_id, runtime);
        let entries = agent_panel_entries_from(&app, &runtime_registry);
        let primary_label = entries[0].primary_label.clone();

        for (_, runtime) in runtime_registry.drain() {
            runtime.shutdown();
        }
        let _ = std::fs::remove_dir_all(root);

        assert_eq!(primary_label, "herdr");
    }

    #[test]
    fn all_workspaces_agent_panel_entries_prefer_agent_names_for_agent_identity() {
        let mut app = crate::app::state::AppState::test_new();
        let workspace = Workspace::test_new("bridge");
        let first_pane = workspace.tabs[0].root_pane;

        app.workspaces = vec![workspace];
        app.ensure_test_terminals();
        let first_terminal_id = app.workspaces[0].tabs[0].panes[&first_pane]
            .attached_terminal_id
            .clone();
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .detected_agent = Some(Agent::Pi);
        app.terminals
            .get_mut(&first_terminal_id)
            .unwrap()
            .set_agent_name("planner".into());
        app.active = Some(0);
        app.selected = 0;

        let entries = agent_panel_entries(&app);
        assert_eq!(entries[0].primary_label, "bridge");
        assert_eq!(entries[0].agent_label.as_deref(), Some("planner"));
    }

    #[test]
    fn expanded_sidebar_sections_handle_tiny_heights() {
        let (ws_area, detail_area) = expanded_sidebar_sections(Rect::new(0, 0, 20, 5), 0.9);

        assert_eq!(ws_area, Rect::new(0, 0, 19, 3));
        assert_eq!(detail_area, Rect::new(0, 3, 19, 2));
    }

    #[test]
    fn sidebar_section_divider_is_hidden_for_tiny_heights() {
        let divider = sidebar_section_divider_rect(Rect::new(0, 0, 20, 5), 0.5);

        assert_eq!(divider, Rect::default());
    }

    #[test]
    fn grouped_child_label_keeps_custom_workspace_name() {
        assert_eq!(
            grouped_child_display_label("renamed issue", Some("worktree/issue-137"), true),
            "renamed issue"
        );
    }

    #[test]
    fn grouped_child_label_uses_short_branch_for_auto_named_workspace() {
        assert_eq!(
            grouped_child_display_label("herdr-issue", Some("worktree/issue-137"), false),
            "issue-137"
        );
    }

    #[test]
    fn workspace_list_truncates_cjk_branch_without_panic() {
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("repo");
        ws.cached_git_branch = Some("feature/中文-分支-644".into());
        app.workspaces = vec![ws];
        app.active = Some(0);
        app.selected = 0;
        app.mode = Mode::Terminal;
        app.view.workspace_card_areas = vec![crate::app::state::WorkspaceCardArea {
            ws_idx: 0,
            rect: Rect::new(0, 1, 15, 2),
            indented: false,
            is_tag_header: false,
        }];

        let mut terminal = Terminal::new(TestBackend::new(15, 6)).expect("test terminal");
        let runtimes = crate::terminal::TerminalRuntimeRegistry::new();

        terminal
            .draw(|frame| {
                render_workspace_list(&app, &runtimes, frame, Rect::new(0, 0, 15, 6), false)
            })
            .expect("workspace list should render");
    }

    fn workspace_with_worktree_space(
        name: &str,
        key: Option<&str>,
        checkout_key: &str,
    ) -> crate::workspace::Workspace {
        let mut ws = crate::workspace::Workspace::test_new(name);
        if let Some(key) = key {
            ws.worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
                key: key.into(),
                label: "herdr".into(),
                repo_root: std::path::PathBuf::from("/repo/herdr"),
                checkout_path: std::path::PathBuf::from(checkout_key),
                is_linked_worktree: name != "main",
            });
        }
        ws
    }

    fn workspace_with_git_space(name: &str, key: &str) -> crate::workspace::Workspace {
        let mut ws = crate::workspace::Workspace::test_new(name);
        ws.cached_git_space = Some(crate::workspace::GitSpaceMetadata {
            key: key.into(),
            checkout_key: format!("/repo/{name}"),
            repo_name: "herdr".into(),
            repo_root: std::path::PathBuf::from(format!("/repo/{name}")),
            is_linked_worktree: false,
        });
        ws
    }

    #[test]
    fn desktop_worktree_tree_aligns_parents_and_marks_children() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
            Workspace::test_new("notes"),
        ];
        app.sidebar_spaces.rows = vec![vec![
            crate::config::SpaceSidebarToken::StateIcon,
            crate::config::SpaceSidebarToken::Workspace,
        ]];
        app.sidebar_spaces.row_gap = 0;
        let area = Rect::new(0, 0, 30, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let list_area = workspace_list_rect(area, app.sidebar_section_split);

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_workspace_list(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    list_area,
                    false,
                )
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let cards = &app.view.workspace_card_areas;
        let parent_name_x = find_symbol_x(buffer, cards[0].rect.y, cards[0].rect.width, "m");
        let plain_name_x = find_symbol_x(buffer, cards[3].rect.y, cards[3].rect.width, "n");
        assert_eq!(parent_name_x, plain_name_x);
        assert_eq!(buffer[(cards[1].rect.x + 3, cards[1].rect.y)].symbol(), "├");
        assert_eq!(buffer[(cards[2].rect.x + 3, cards[2].rect.y)].symbol(), "└");
        assert_eq!(
            buffer[(cards[0].rect.x + cards[0].rect.width - 1, cards[0].rect.y)].symbol(),
            "▾"
        );
    }

    #[test]
    fn desktop_worktree_connector_uses_full_list_at_viewport_boundary() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
        ];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        app.sidebar_spaces.row_gap = 0;
        let area = Rect::new(0, 0, 30, 10);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        assert_eq!(app.view.workspace_card_areas.len(), 2);
        let list_area = workspace_list_rect(area, app.sidebar_section_split);

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_workspace_list(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    list_area,
                    false,
                )
            })
            .unwrap();

        let child = app.view.workspace_card_areas[1];
        assert_eq!(
            terminal.backend().buffer()[(child.rect.x + 3, child.rect.y)].symbol(),
            "├"
        );
    }

    #[test]
    fn parent_workspace_row_stays_clickable_when_grouped() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];
        app.sidebar_spaces.row_gap = 1;

        let (cards, headers) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 20));

        assert!(headers.is_empty());
        assert_eq!(cards[0].ws_idx, 0);
        assert!(!cards[0].indented);
        assert_eq!(cards[1].ws_idx, 1);
        assert!(cards[1].indented);
        assert_eq!(cards[1].rect.y, cards[0].rect.y + cards[0].rect.height);
    }

    #[test]
    fn space_row_gap_preserves_compact_worktree_children() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
            Workspace::test_new("notes"),
        ];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        app.sidebar_spaces.row_gap = 2;

        let (spacious, _) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 30));
        assert_eq!(
            spacious[1].rect.y,
            spacious[0].rect.y + spacious[0].rect.height
        );
        assert_eq!(
            spacious[2].rect.y,
            spacious[1].rect.y + spacious[1].rect.height
        );
        assert_eq!(
            spacious[3].rect.y,
            spacious[2].rect.y + spacious[2].rect.height + 2
        );
        let spacious_metrics = workspace_list_scroll_metrics(&app, Rect::new(0, 0, 30, 7));
        assert_eq!(spacious_metrics.viewport_rows, 3);
        assert_eq!(spacious_metrics.max_offset_from_bottom, 2);

        app.sidebar_spaces.row_gap = 0;
        let (packed, _) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 30));
        assert!(packed
            .windows(2)
            .all(|pair| pair[1].rect.y == pair[0].rect.y + pair[0].rect.height));
        let packed_metrics = workspace_list_scroll_metrics(&app, Rect::new(0, 0, 30, 7));
        assert_eq!(packed_metrics.viewport_rows, 4);
        assert_eq!(packed_metrics.max_offset_from_bottom, 0);
    }

    #[test]
    fn packed_workspace_drag_indicator_overlays_an_internal_boundary() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("a"),
            Workspace::test_new("b"),
            Workspace::test_new("c"),
        ];
        app.sidebar_spaces.rows = vec![vec![crate::config::SpaceSidebarToken::Workspace]];
        app.sidebar_spaces.row_gap = 0;
        let area = Rect::new(0, 0, 30, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let list_area = workspace_list_rect(area, app.sidebar_section_split);
        let indicator_row = workspace_drop_indicator_row(
            &app,
            &app.view.workspace_card_areas,
            list_area,
            crate::app::state::WorkspaceDropTarget::Before(2),
        )
        .unwrap();
        assert_eq!(indicator_row, app.view.workspace_card_areas[1].rect.y);
        app.drag = Some(crate::app::state::DragState {
            target: crate::app::state::DragTarget::WorkspaceReorder {
                source_id: 0,
                source_ws_idx: 0,
                drop_target: Some(crate::app::state::WorkspaceDropTarget::Before(2)),
            },
        });

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| {
                render_workspace_list(
                    &app,
                    &TerminalRuntimeRegistry::new(),
                    frame,
                    list_area,
                    false,
                )
            })
            .unwrap();

        assert_eq!(
            terminal.backend().buffer()[(list_area.x, indicator_row)].symbol(),
            "─"
        );
    }

    #[test]
    fn linked_only_worktree_members_do_not_form_parentless_group() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            workspace_with_worktree_space("review", Some("repo-key"), "/repo/herdr-review"),
        ];

        let entries = workspace_list_entries(&app);

        assert_eq!(
            entries,
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false
                },
            ]
        );
    }

    #[test]
    fn compact_space_group_scroll_clamps_when_all_entries_fit() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("one", Some("repo-key"), "/repo/herdr-one"),
            workspace_with_worktree_space("two", Some("repo-key"), "/repo/herdr-two"),
        ];
        let area = Rect::new(0, 0, 30, 20);
        app.workspace_scroll = normalized_workspace_scroll(&app, area, 2);

        let (cards, headers) = compute_workspace_list_areas(&app, area);

        assert!(headers.is_empty());
        assert_eq!(app.workspace_scroll, 0);
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[2].ws_idx, 2);
    }

    #[test]
    fn workspace_scroll_metrics_count_display_entries_not_raw_workspaces() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            Workspace::test_new("notes"),
        ];
        for workspace in &mut app.workspaces {
            workspace.cached_git_branch = Some("main".into());
        }
        app.collapsed_space_keys.insert("repo-key".into());
        app.active = None;
        app.mode = Mode::Terminal;

        let ws_area = Rect::new(0, 0, 30, 6);
        let metrics = workspace_list_scroll_metrics(&app, ws_area);

        assert_eq!(metrics.viewport_rows, 1);
        assert_eq!(metrics.max_offset_from_bottom, 1);
        assert_eq!(metrics.offset_from_bottom, 1);
    }

    #[test]
    fn workspace_scroll_offset_applies_to_group_children() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
            Workspace::test_new("notes"),
        ];
        app.collapsed_space_keys.insert("repo-key".into());
        app.active = None;
        app.mode = Mode::Terminal;
        app.workspace_scroll = 1;

        let (cards, headers) = compute_workspace_list_areas(&app, Rect::new(0, 0, 30, 12));

        assert!(headers.is_empty());
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].ws_idx, 2);
    }

    #[test]
    fn workspace_list_entries_group_multiple_workspaces_in_same_git_space() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: true,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_group_non_contiguous_explicit_members() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_git_space("normal", "other-key"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 2,
                    indented: true,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_do_not_group_normal_git_workspaces() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_git_space("one", "repo-key"),
            workspace_with_git_space("two", "repo-key"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_do_not_auto_attach_normal_git_workspace_to_group() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_git_space("scratch", "repo-key"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 2,
                    indented: true,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn workspace_list_entries_leave_single_git_and_non_git_workspaces_flat() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_git_space("one", "repo-key"),
            workspace_with_worktree_space("notes", None, "/notes"),
        ];

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: false,
                },
            ]
        );
    }

    #[test]
    fn collapsed_group_hides_inactive_children_but_keeps_active_visible() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];
        app.active = Some(1);
        app.mode = Mode::Terminal;
        app.collapsed_space_keys.insert("repo-key".into());

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: true,
                },
            ]
        );

        app.active = None;
        app.mode = Mode::Terminal;
        assert_eq!(
            workspace_list_entries(&app),
            vec![WorkspaceListEntry::Workspace {
                ws_idx: 0,
                indented: false,
            }]
        );
    }

    #[test]
    fn collapsed_group_keeps_selected_child_visible_in_navigate_mode() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            workspace_with_worktree_space("main", Some("repo-key"), "/repo/herdr"),
            workspace_with_worktree_space("issue", Some("repo-key"), "/repo/herdr-issue"),
        ];
        app.mode = Mode::Navigate;
        app.selected = 1;
        app.active = Some(1);
        app.collapsed_space_keys.insert("repo-key".into());

        assert_eq!(
            workspace_list_entries(&app),
            vec![
                WorkspaceListEntry::Workspace {
                    ws_idx: 0,
                    indented: false,
                },
                WorkspaceListEntry::Workspace {
                    ws_idx: 1,
                    indented: true,
                },
            ]
        );
    }

    #[test]
    fn group_entries_by_tag_orders_groups_by_first_appearance_then_untagged() {
        let tags = vec![
            Some("research".to_string()),
            None,
            Some("teaching".to_string()),
            Some("research".to_string()),
            None,
        ];

        let grouping = group_entries_by_tag(&tags);

        assert_eq!(
            grouping.groups,
            vec![
                TagGroup {
                    tag: "research".to_string(),
                    member_entry_indices: vec![0, 3],
                },
                TagGroup {
                    tag: "teaching".to_string(),
                    member_entry_indices: vec![2],
                },
            ]
        );
        assert_eq!(grouping.ungrouped, vec![1, 4]);
    }

    #[test]
    fn group_entries_by_tag_treats_empty_string_as_untagged() {
        let tags = vec![Some(String::new()), Some("a".to_string())];

        let grouping = group_entries_by_tag(&tags);

        assert_eq!(grouping.ungrouped, vec![0]);
        assert_eq!(grouping.groups.len(), 1);
        assert_eq!(grouping.groups[0].tag, "a");
    }

    #[test]
    fn group_entries_by_tag_without_tags_is_all_ungrouped() {
        let tags = vec![None, None, None];

        let grouping = group_entries_by_tag(&tags);

        assert!(grouping.groups.is_empty());
        assert_eq!(grouping.ungrouped, vec![0, 1, 2]);
    }

    fn tagged_workspace(name: &str, tag: Option<&str>) -> Workspace {
        let mut ws = Workspace::test_new(name);
        ws.tag = tag.map(str::to_string);
        ws
    }

    #[test]
    fn tag_layout_rows_places_header_before_each_group_and_untagged_last() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            tagged_workspace("a", Some("research")),
            tagged_workspace("b", None),
            tagged_workspace("c", Some("research")),
            tagged_workspace("d", Some("teaching")),
        ];

        let rows = tag_layout_rows(&app);

        assert_eq!(
            rows,
            vec![
                TagLayoutRow::Header {
                    tag: "research".to_string(),
                    count: 2,
                    collapsed: false,
                    first_ws_idx: 0,
                },
                TagLayoutRow::Entry {
                    entry: WorkspaceListEntry::Workspace {
                        ws_idx: 0,
                        indented: false,
                    },
                    grouped: true,
                },
                TagLayoutRow::Entry {
                    entry: WorkspaceListEntry::Workspace {
                        ws_idx: 2,
                        indented: false,
                    },
                    grouped: true,
                },
                TagLayoutRow::Header {
                    tag: "teaching".to_string(),
                    count: 1,
                    collapsed: false,
                    first_ws_idx: 3,
                },
                TagLayoutRow::Entry {
                    entry: WorkspaceListEntry::Workspace {
                        ws_idx: 3,
                        indented: false,
                    },
                    grouped: true,
                },
                TagLayoutRow::Entry {
                    entry: WorkspaceListEntry::Workspace {
                        ws_idx: 1,
                        indented: false,
                    },
                    grouped: false,
                },
            ]
        );
    }

    #[test]
    fn collapsed_tag_group_hides_members_but_keeps_header() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            tagged_workspace("a", Some("research")),
            tagged_workspace("c", Some("research")),
            tagged_workspace("b", None),
        ];
        app.collapsed_space_keys
            .insert(tag_collapse_key("research"));

        let rows = tag_layout_rows(&app);

        assert_eq!(
            rows,
            vec![
                TagLayoutRow::Header {
                    tag: "research".to_string(),
                    count: 2,
                    collapsed: true,
                    first_ws_idx: 0,
                },
                TagLayoutRow::Entry {
                    entry: WorkspaceListEntry::Workspace {
                        ws_idx: 2,
                        indented: false,
                    },
                    grouped: false,
                },
            ]
        );
        assert_eq!(workspace_display_order(&app), vec![2]);
    }

    #[test]
    fn grouped_member_card_indents_two_cells_untagged_stays_at_column_zero() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            tagged_workspace("a", Some("research")),
            tagged_workspace("b", None),
        ];

        let area = Rect::new(0, 0, 30, 20);
        let body =
            workspace_list_body_rect(workspace_list_rect(area, app.sidebar_section_split), false);
        let (cards, _) = compute_workspace_list_areas(&app, area);

        // header (ws 0), grouped member (ws 0), untagged remainder (ws 1).
        let grouped = cards
            .iter()
            .find(|c| c.ws_idx == 0 && !c.is_tag_header)
            .unwrap();
        let untagged = cards.iter().find(|c| c.ws_idx == 1).unwrap();

        assert_eq!(grouped.rect.x, body.x + 2);
        assert_eq!(grouped.rect.width, body.width - 2);
        assert_eq!(untagged.rect.x, body.x);
        assert_eq!(untagged.rect.width, body.width);
    }

    #[test]
    fn tag_color_is_stable_for_the_same_tag() {
        let p = Palette::catppuccin();
        assert_eq!(tag_color("research", &p), tag_color("research", &p));
        assert_eq!(tag_color("teaching", &p), tag_color("teaching", &p));
    }

    #[test]
    fn tag_color_differs_for_two_known_different_tags() {
        let p = Palette::catppuccin();
        // "research" hashes to bucket 3 (yellow), "teaching" to bucket 1 (teal),
        // so the two land on different palette colors.
        assert_ne!(tag_color("research", &p), tag_color("teaching", &p));
    }

    #[test]
    fn tag_color_never_returns_red() {
        let p = Palette::catppuccin();
        for tag in [
            "research", "teaching", "prepara", "a", "b", "c", "d", "e", "zzz", "ops", "docs",
            "video",
        ] {
            assert_ne!(tag_color(tag, &p), p.red);
        }
    }

    #[test]
    fn tag_color_override_beats_the_hash_default() {
        let p = Palette::catppuccin();
        let mut overrides = std::collections::HashMap::new();
        // "research" hashes to a non-mauve default; an explicit mauve override wins.
        overrides.insert("research".to_string(), TagAccent::Mauve.id().to_string());
        assert_eq!(
            tag_color_with_overrides("research", &overrides, &p),
            p.mauve
        );
        // A tag without an override keeps its stable-hash color.
        assert_eq!(
            tag_color_with_overrides("teaching", &overrides, &p),
            tag_color("teaching", &p)
        );
    }

    #[test]
    fn tag_color_override_with_unknown_id_falls_back_to_hash_default() {
        let p = Palette::catppuccin();
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("research".to_string(), "chartreuse".to_string());
        assert_eq!(
            tag_color_with_overrides("research", &overrides, &p),
            tag_color("research", &p)
        );
    }

    #[test]
    fn tag_accent_ids_round_trip_and_default_never_mauve() {
        for accent in TagAccent::ALL {
            assert_eq!(TagAccent::from_id(accent.id()), Some(accent));
        }
        // The hash default reaches only the historical four; mauve is pick-only.
        for tag in ["research", "teaching", "prepara", "a", "b", "zzz", "docs"] {
            assert_ne!(default_tag_accent(tag), TagAccent::Mauve);
        }
    }

    #[test]
    fn sort_tag_groups_name_orders_alphabetically_case_insensitive() {
        let mut groups = vec![
            TagGroup {
                tag: "teaching".into(),
                member_entry_indices: vec![0],
            },
            TagGroup {
                tag: "Research".into(),
                member_entry_indices: vec![1],
            },
            TagGroup {
                tag: "prepara".into(),
                member_entry_indices: vec![2],
            },
        ];
        sort_tag_groups(&mut groups, crate::config::TagSortMode::Name, &[]);
        let names: Vec<_> = groups.iter().map(|g| g.tag.as_str()).collect();
        assert_eq!(names, vec!["prepara", "Research", "teaching"]);
    }

    #[test]
    fn sort_tag_groups_manual_follows_order_then_first_appearance_for_unknowns() {
        let mut groups = vec![
            TagGroup {
                tag: "research".into(),
                member_entry_indices: vec![0],
            },
            TagGroup {
                tag: "teaching".into(),
                member_entry_indices: vec![1],
            },
            TagGroup {
                tag: "prepara".into(),
                member_entry_indices: vec![2],
            },
        ];
        // Manual order lists teaching then prepara; research is unknown, so it keeps
        // its first-appearance slot after the listed ones.
        let manual = vec!["teaching".to_string(), "prepara".to_string()];
        sort_tag_groups(&mut groups, crate::config::TagSortMode::Manual, &manual);
        let names: Vec<_> = groups.iter().map(|g| g.tag.as_str()).collect();
        assert_eq!(names, vec!["teaching", "prepara", "research"]);
    }

    #[test]
    fn sort_tag_groups_first_appearance_leaves_input_order() {
        let mut groups = vec![
            TagGroup {
                tag: "teaching".into(),
                member_entry_indices: vec![0],
            },
            TagGroup {
                tag: "research".into(),
                member_entry_indices: vec![1],
            },
        ];
        sort_tag_groups(
            &mut groups,
            crate::config::TagSortMode::FirstAppearance,
            &["research".to_string()],
        );
        let names: Vec<_> = groups.iter().map(|g| g.tag.as_str()).collect();
        assert_eq!(names, vec!["teaching", "research"]);
    }

    #[test]
    fn tag_layout_rows_honor_manual_order_in_both_the_header_and_members() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            tagged_workspace("a", Some("research")),
            tagged_workspace("b", Some("teaching")),
        ];
        app.sidebar_spaces.tag_sort = crate::config::TagSortMode::Manual;
        app.tag_order = vec!["teaching".to_string(), "research".to_string()];

        let headers: Vec<_> = tag_layout_rows(&app)
            .into_iter()
            .filter_map(|row| match row {
                TagLayoutRow::Header { tag, .. } => Some(tag),
                TagLayoutRow::Entry { .. } => None,
            })
            .collect();
        assert_eq!(
            headers,
            vec!["teaching".to_string(), "research".to_string()]
        );
        assert_eq!(
            ordered_tag_names(&app),
            vec!["teaching".to_string(), "research".to_string()]
        );
    }

    #[test]
    fn tag_layout_rows_name_mode_sorts_headers_alphabetically() {
        let mut app = AppState::test_new();
        app.workspaces = vec![
            tagged_workspace("a", Some("teaching")),
            tagged_workspace("b", Some("research")),
        ];
        app.sidebar_spaces.tag_sort = crate::config::TagSortMode::Name;

        assert_eq!(
            ordered_tag_names(&app),
            vec!["research".to_string(), "teaching".to_string()]
        );
    }

    #[test]
    fn need_rollup_empty_is_none() {
        assert_eq!(need_rollup(&[]), None);
    }

    #[test]
    fn need_rollup_orders_blocked_over_needs_you_over_working_over_idle() {
        // Blocked wins over everything.
        assert_eq!(
            need_rollup(&[
                (AgentState::Idle, true),
                (AgentState::Working, true),
                (AgentState::Idle, false),
                (AgentState::Blocked, true),
            ]),
            Some(NeedLevel::Blocked)
        );
        // NeedsYou (idle + unseen) wins over working and idle-seen.
        assert_eq!(
            need_rollup(&[
                (AgentState::Idle, true),
                (AgentState::Working, true),
                (AgentState::Idle, false),
            ]),
            Some(NeedLevel::NeedsYou)
        );
        // Working wins over idle-seen.
        assert_eq!(
            need_rollup(&[(AgentState::Idle, true), (AgentState::Working, true)]),
            Some(NeedLevel::Working)
        );
        // Idle-seen is the floor.
        assert_eq!(
            need_rollup(&[(AgentState::Idle, true)]),
            Some(NeedLevel::Idle)
        );
        // Unknown-only rolls up to nothing.
        assert_eq!(need_rollup(&[(AgentState::Unknown, true)]), None);
    }

    /// Build an app whose workspaces each own one detected agent, tagged as
    /// given, with a state/seen pair, so the agents panel produces one entry per
    /// workspace. Returns the app ready for `agent_panel_entries`.
    fn tagged_agent_app(specs: &[(&str, Option<&str>, AgentState, bool)]) -> AppState {
        let mut app = AppState::test_new();
        app.workspaces = specs
            .iter()
            .map(|(name, tag, _, _)| tagged_workspace(name, *tag))
            .collect();
        app.ensure_test_terminals();
        app.active = None;
        app.mode = Mode::Terminal;
        for (ws_idx, (_, _, state, seen)) in specs.iter().enumerate() {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            let terminal = app.terminals.get_mut(&terminal_id).unwrap();
            terminal.detected_agent = Some(Agent::Claude);
            terminal.state = *state;
            app.workspaces[ws_idx].tabs[0]
                .panes
                .get_mut(&pane)
                .unwrap()
                .seen = *seen;
        }
        app
    }

    #[test]
    fn agent_panel_display_rows_group_by_inherited_tag_first_appearance_untagged_last() {
        let app = tagged_agent_app(&[
            ("a", Some("research"), AgentState::Idle, true),
            ("b", None, AgentState::Idle, true),
            ("c", Some("research"), AgentState::Idle, true),
            ("d", Some("teaching"), AgentState::Idle, true),
        ]);
        let entries = agent_panel_entries(&app);
        let rows = agent_panel_display_rows(&app, &entries);

        // research header, its two members (entry idx 0 and 2), teaching header,
        // its member (entry idx 3), then the untagged remainder (entry idx 1).
        let header_tags: Vec<(&str, usize)> = rows
            .iter()
            .filter_map(|row| match row {
                AgentPanelRow::Header { tag, count, .. } => Some((tag.as_str(), *count)),
                AgentPanelRow::Entry { .. } => None,
            })
            .collect();
        assert_eq!(header_tags, vec![("research", 2), ("teaching", 1)]);

        let entry_order: Vec<usize> = rows
            .iter()
            .filter_map(|row| match row {
                AgentPanelRow::Entry { entry_idx } => Some(*entry_idx),
                AgentPanelRow::Header { .. } => None,
            })
            .collect();
        // Members are pulled under their header in original order; untagged last.
        assert_eq!(entry_order, vec![0, 2, 3, 1]);
    }

    #[test]
    fn agent_panel_no_tags_yields_plain_entry_rows_without_headers() {
        let app = tagged_agent_app(&[
            ("a", None, AgentState::Idle, true),
            ("b", None, AgentState::Working, true),
        ]);
        let entries = agent_panel_entries(&app);
        let rows = agent_panel_display_rows(&app, &entries);

        assert_eq!(
            rows,
            vec![
                AgentPanelRow::Entry { entry_idx: 0 },
                AgentPanelRow::Entry { entry_idx: 1 },
            ]
        );
        assert!(!rows
            .iter()
            .any(|row| matches!(row, AgentPanelRow::Header { .. })));
    }

    #[test]
    fn collapsed_agent_tag_group_hides_members_but_keeps_header() {
        let mut app = tagged_agent_app(&[
            ("a", Some("research"), AgentState::Idle, true),
            ("c", Some("research"), AgentState::Idle, true),
            ("b", None, AgentState::Idle, true),
        ]);
        app.collapsed_space_keys
            .insert(agent_tag_collapse_key("research"));
        let entries = agent_panel_entries(&app);
        let rows = agent_panel_display_rows(&app, &entries);

        assert_eq!(
            rows,
            vec![
                AgentPanelRow::Header {
                    tag: "research".to_string(),
                    count: 2,
                    collapsed: true,
                    need: Some(NeedLevel::Idle),
                },
                AgentPanelRow::Entry { entry_idx: 2 },
            ]
        );
    }

    #[test]
    fn agent_tag_header_rolls_up_the_strongest_member_state() {
        // One research agent is blocked, the other idle-seen: the header's rolled
        // up need is Blocked, the strongest across the members. A separate teaching
        // agent that is idle-and-unseen rolls up to NeedsYou.
        let app = tagged_agent_app(&[
            ("a", Some("research"), AgentState::Idle, true),
            ("b", Some("research"), AgentState::Blocked, true),
            ("c", Some("teaching"), AgentState::Idle, false),
        ]);
        let entries = agent_panel_entries(&app);
        let rows = agent_panel_display_rows(&app, &entries);

        let needs: Vec<(&str, Option<NeedLevel>)> = rows
            .iter()
            .filter_map(|row| match row {
                AgentPanelRow::Header { tag, need, .. } => Some((tag.as_str(), *need)),
                AgentPanelRow::Entry { .. } => None,
            })
            .collect();
        assert_eq!(
            needs,
            vec![
                ("research", Some(NeedLevel::Blocked)),
                ("teaching", Some(NeedLevel::NeedsYou)),
            ]
        );
    }

    #[test]
    fn agent_tag_group_collapse_key_is_distinct_from_the_spaces_key() {
        assert_eq!(agent_tag_collapse_key("research"), "agent-tag:research");
        assert_ne!(
            agent_tag_collapse_key("research"),
            tag_collapse_key("research")
        );
    }

    #[test]
    fn agent_panel_renders_tag_headers_above_their_member_rows() {
        let app = tagged_agent_app(&[
            ("alpha", Some("research"), AgentState::Idle, true),
            ("bravo", None, AgentState::Idle, true),
        ]);
        let area = Rect::new(0, 0, 30, 24);
        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let (_, agent_area) = expanded_sidebar_sections(area, app.sidebar_section_split);
        let body = agent_panel_body_rect(agent_area, false);

        // The first body row is the research header (chevron + tag name), the
        // agent row for "alpha" follows below it.
        let header = row_text(buffer, body.y, area.width);
        assert!(
            header.contains("research"),
            "expected tag header, got {header:?}"
        );
        assert!(
            header.contains("▾"),
            "expected open chevron, got {header:?}"
        );
        let member = row_text(buffer, body.y + 1, area.width);
        assert!(
            member.contains("alpha"),
            "expected member row, got {member:?}"
        );
    }

    #[test]
    fn workspace_rows_carry_a_need_edge_colored_by_their_own_rollup() {
        // Two top-level spaces: the first finished and unseen (needs you), the
        // second actively working. The row's first cell should carry the green
        // edge for the first and a blank cell for the second, mirroring how the
        // tag-group headers color their leading edge.
        let mut app = crate::app::state::AppState::test_new();
        app.workspaces = vec![
            Workspace::test_new("needsyou"),
            Workspace::test_new("working"),
        ];
        app.ensure_test_terminals();
        app.active = None;
        app.mode = Mode::Terminal;

        let set_state = |app: &mut crate::app::state::AppState, ws_idx: usize, state, seen| {
            let pane = app.workspaces[ws_idx].tabs[0].root_pane;
            let terminal_id = app.workspaces[ws_idx].tabs[0].panes[&pane]
                .attached_terminal_id
                .clone();
            app.terminals.get_mut(&terminal_id).unwrap().state = state;
            app.workspaces[ws_idx].tabs[0]
                .panes
                .get_mut(&pane)
                .unwrap()
                .seen = seen;
        };
        // Idle + unseen rolls up to NeedsYou (green); Working rolls up to no edge.
        set_state(&mut app, 0, AgentState::Idle, false);
        set_state(&mut app, 1, AgentState::Working, true);

        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let needs_you = app.view.workspace_card_areas[0].rect;
        let working = app.view.workspace_card_areas[1].rect;

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let needs_you_edge = &buffer[(needs_you.x, needs_you.y)];
        assert_eq!(needs_you_edge.symbol(), "▎");
        assert_eq!(needs_you_edge.style().fg, Some(app.palette.green));

        let working_edge = &buffer[(working.x, working.y)];
        assert_eq!(working_edge.symbol(), " ");
        assert_ne!(working_edge.style().fg, Some(app.palette.green));
    }

    #[test]
    fn top_level_space_with_a_branch_draws_its_second_line() {
        // Regression guard: a top-level (ungrouped, non-worktree-child) space whose
        // config carries a branch row must render that branch on a second line. The
        // whole chain runs for real - compute_workspace_card_areas derives the card
        // height from space_rows, and render_workspace_list draws each resolved row -
        // so a card truncated to one line (the tag-grouping regression) is caught here.
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("repo");
        ws.cached_git_branch = Some("feature-branch".into());
        app.workspaces = vec![ws];
        app.ensure_test_terminals();
        app.active = None;
        app.mode = Mode::Terminal;

        // The default spaces layout is [[state_icon, workspace], [branch, git_status]].
        assert_eq!(app.sidebar_spaces.rows.len(), 2);

        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let card = app.view.workspace_card_areas[0].rect;
        assert!(
            card.height >= 2,
            "a space with a branch must get a card at least two rows tall, got {}",
            card.height
        );

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let second_line = row_text(buffer, card.y + 1, area.width);
        assert!(
            second_line.contains("feature-branch"),
            "second card line should carry the branch, got {second_line:?}"
        );
    }

    #[test]
    fn felipe_three_row_space_layout_draws_branch_and_usage_lines() {
        // Reproduces Felipe's live [ui.sidebar.spaces] layout verbatim -
        // [[state_icon, workspace(styled)], [branch, git_status], [$usage]] - for a
        // plain top-level space carrying a branch and a $usage token. All three rows
        // must survive: the card is three tall and the branch and usage each land on
        // their own line. Guards the post-swap regression where every space collapsed
        // to a single line.
        let mut app = crate::app::state::AppState::test_new();
        app.sidebar_spaces = crate::config::SpacesSidebarConfig {
            rows: vec![
                vec![
                    crate::config::SpaceSidebarToken::StateIcon,
                    crate::config::SpaceSidebarToken::Styled {
                        token: Box::new(crate::config::SpaceSidebarToken::Workspace),
                        style: crate::config::SidebarTokenStyle::default(),
                    },
                ],
                vec![
                    crate::config::SpaceSidebarToken::Branch,
                    crate::config::SpaceSidebarToken::GitStatus,
                ],
                vec![crate::config::SpaceSidebarToken::Custom("usage".into())],
            ],
            row_gap: 0,
            tag_sort: crate::config::TagSortMode::default(),
        };
        let mut ws = Workspace::test_new("repo");
        ws.cached_git_branch = Some("mainline".into());
        ws.metadata_tokens.patch(
            std::collections::HashMap::from([("usage".to_string(), Some("42%".to_string()))]),
            None,
            std::time::Instant::now(),
        );
        app.workspaces = vec![ws];
        app.ensure_test_terminals();
        app.active = None;
        app.mode = Mode::Terminal;

        let area = Rect::new(0, 0, 26, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        let card = app.view.workspace_card_areas[0].rect;
        assert_eq!(
            card.height, 3,
            "the three-row layout must yield a three-row-tall card"
        );

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let branch_line = row_text(buffer, card.y + 1, area.width);
        assert!(
            branch_line.contains("mainline"),
            "second card line should carry the branch, got {branch_line:?}"
        );
        let usage_line = row_text(buffer, card.y + 2, area.width);
        assert!(
            usage_line.contains("42%"),
            "third card line should carry the usage token, got {usage_line:?}"
        );
    }

    #[test]
    fn tagged_space_still_draws_its_branch_line_under_the_group_indent() {
        // A tagged workspace routes through tag_layout_rows and gets the two-cell
        // group indent (compute_workspace_list_areas shifts a grouped card x+2,
        // width-2). Confirm that indent does not swallow the second (branch) line:
        // the card stays two rows tall and the branch still renders on line two,
        // shifted right by the indent.
        let mut app = crate::app::state::AppState::test_new();
        let mut ws = Workspace::test_new("repo");
        ws.cached_git_branch = Some("mainline".into());
        ws.set_tag(Some("teaching".into()));
        app.workspaces = vec![ws];
        app.ensure_test_terminals();
        app.active = None;
        app.mode = Mode::Terminal;

        assert!(has_tag_groups(&app), "the workspace should be tagged");

        let area = Rect::new(0, 0, 30, 20);
        app.view.workspace_card_areas = compute_workspace_card_areas(&app, area);
        // Card 0 is the tag header; card 1 is the workspace itself.
        let ws_card = app
            .view
            .workspace_card_areas
            .iter()
            .find(|card| !card.is_tag_header)
            .expect("a workspace card")
            .rect;
        assert_eq!(
            ws_card.height, 2,
            "the two-row layout must survive the group indent"
        );

        let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
        terminal
            .draw(|frame| render_sidebar(&app, &TerminalRuntimeRegistry::new(), frame, area))
            .unwrap();
        let buffer = terminal.backend().buffer();

        let branch_line = row_text(buffer, ws_card.y + 1, area.width);
        assert!(
            branch_line.contains("mainline"),
            "second card line should carry the branch even when grouped, got {branch_line:?}"
        );
    }
}
