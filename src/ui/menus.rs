use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

use super::keybind_help::prefix_which_key_groups;
use super::widgets::{centered_popup_rect, panel_contrast_fg, render_panel_shell};
use crate::app::AppState;

fn prefix_rhs_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings
        .prefix_rhs_label()
        .unwrap_or_else(|| "unset".to_string())
}

fn keybind_label(bindings: &crate::config::ActionKeybinds) -> String {
    bindings.label().unwrap_or_else(|| "unset".to_string())
}

fn render_bottom_bar(frame: &mut Frame, area: Rect, line: Line<'_>, bg: ratatui::style::Color) {
    frame.render_widget(Clear, area);
    let buf = frame.buffer_mut();
    for x in area.x..area.x + area.width {
        buf[(x, area.y)].set_style(Style::default().bg(bg));
    }
    frame.render_widget(Paragraph::new(line), area);
}

pub(super) fn render_prefix_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    // Once the prefix has been held past the which-key delay, or an unbound key was
    // pressed, expand into the full which-key popup listing every continuation. The
    // slim bottom bar stays the instant, low-noise hint for the muscle-memory case.
    // The popup centers on the terminal surface, not the mode-bar row: with a
    // bottom tab bar `area` is that single row and could never hold a card.
    if app.prefix_which_key_expanded {
        render_prefix_which_key_popup(app, frame, app.view.terminal_area, area);
        return;
    }

    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);

    let workspace_picker = prefix_rhs_label(&app.keybinds.workspace_picker);
    let help = prefix_rhs_label(&app.keybinds.help);
    let prefix = crate::config::format_key_combo((app.prefix_code, app.prefix_mods));

    let line = Line::from(vec![
        Span::styled(" PREFIX ", mode_style),
        Span::raw(" "),
        Span::styled("esc", key),
        Span::styled(" cancel  ", dim),
        Span::styled(prefix, key),
        Span::styled(" send prefix  ", dim),
        Span::styled(workspace_picker, key),
        Span::styled(" workspace nav  ", dim),
        Span::styled(help, key),
        Span::styled(" keybinds", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

/// The which-key card: after the prefix chord, a rounded panel listing every
/// bound continuation (built-ins plus `[[keys.command]]` customs with their
/// descriptions) so nothing rests on memory. Laid out in as many columns as fit
/// so the whole list is visible on ordinary terminals; when even that cannot fit,
/// the last visible row becomes a "+N more" count rather than a silent cut.
/// `area` hosts the centered card; `bar_area` hosts the slim fallback bar when
/// the card cannot fit at all.
fn render_prefix_which_key_popup(app: &AppState, frame: &mut Frame, area: Rect, bar_area: Rect) {
    let groups = prefix_which_key_groups(app);

    // Flatten into rendered lines: a bold heading per group, then one row per
    // binding ("chord  description"), a blank line between groups.
    let heading_style = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let key_style = Style::default()
        .fg(app.palette.mauve)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(app.palette.text);
    let dim_style = Style::default().fg(app.palette.overlay0);

    let chord_width = groups
        .iter()
        .flat_map(|(_, entries)| entries.iter().map(|(chord, _)| chord.chars().count()))
        .max()
        .unwrap_or(1);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (group, entries) in &groups {
        if !lines.is_empty() {
            lines.push(Line::raw(""));
        }
        lines.push(Line::from(Span::styled(format!(" {group}"), heading_style)));
        for (chord, description) in entries {
            let padded = format!(" {:<width$} ", chord, width = chord_width);
            lines.push(Line::from(vec![
                Span::styled(padded, key_style),
                Span::styled(description.clone().into_owned(), label_style),
            ]));
        }
    }
    if lines.is_empty() {
        render_prefix_hint_bar(app, frame, bar_area);
        return;
    }

    // Column layout. A column is as wide as the widest line plus a two-cell
    // gutter; the popup carries a border (2 rows/cols) and a one-row title.
    let title = " prefix ";
    let title_hint = "next key";
    let total = lines.len();
    let widest = lines
        .iter()
        .map(ratatui::text::Line::width)
        .max()
        .unwrap_or(0)
        .max(title.len() + title_hint.len())
        .max(1);
    let col_w = widest + 2;

    let avail_w = area.width.saturating_sub(4) as usize;
    let avail_h = area.height.saturating_sub(2) as usize;
    let max_body_rows = avail_h.saturating_sub(3);
    if max_body_rows == 0 || avail_w < 4 {
        render_prefix_hint_bar(app, frame, bar_area);
        return;
    }
    let max_cols = (avail_w / col_w).max(1);
    let cols = total.div_ceil(max_body_rows).min(max_cols);
    let body_rows = total.div_ceil(cols).min(max_body_rows);
    let shown = (cols * body_rows).min(total);
    if shown < total {
        // Not everything fits even in columns: the last visible row says how much
        // is hidden instead of cutting the list silently.
        let hidden = total - shown + 1;
        lines[shown - 1] = Line::from(Span::styled(format!(" +{hidden} more"), dim_style));
    }

    let popup_w = ((cols * col_w) as u16).min(area.width);
    let popup_h = ((body_rows + 3) as u16).min(area.height);
    let Some(popup) = centered_popup_rect(area, popup_w, popup_h) else {
        // Too small for the card: fall back to the slim bar so the mode is never invisible.
        render_prefix_hint_bar(app, frame, bar_area);
        return;
    };
    let Some(inner) = render_panel_shell(frame, popup, app.palette.surface1, app.palette.panel_bg)
    else {
        render_prefix_hint_bar(app, frame, bar_area);
        return;
    };
    if inner.height < 2 || inner.width < 2 {
        return;
    }

    let title_area = Rect::new(inner.x, inner.y, inner.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                title,
                Style::default()
                    .fg(app.palette.text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(title_hint, dim_style),
        ])),
        title_area,
    );

    let body_y = inner.y + 1;
    let body_height = (inner.height.saturating_sub(1) as usize).min(body_rows);
    if body_height == 0 {
        return;
    }
    lines.truncate(shown);
    for col in 0..cols {
        let start = col * body_rows;
        if start >= lines.len() {
            break;
        }
        let end = (start + body_rows).min(lines.len());
        let x = inner.x + (col * col_w) as u16;
        if x >= inner.x + inner.width {
            break;
        }
        let width = (widest as u16).min(inner.width.saturating_sub(x - inner.x));
        let column_area = Rect::new(x, body_y, width, body_height as u16);
        frame.render_widget(Paragraph::new(lines[start..end].to_vec()), column_area);
    }
}

fn render_prefix_hint_bar(app: &AppState, frame: &mut Frame, area: Rect) {
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let line = Line::from(vec![
        Span::styled(" PREFIX ", mode_style),
        Span::raw(" "),
        Span::styled("esc", key),
        Span::styled(" cancel", dim),
    ]);
    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_copy_mode_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);
    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);

    let Some(copy_mode) = app.copy_mode.as_ref() else {
        return;
    };
    let line = if let Some(prompt) = copy_mode.search.prompt.as_ref() {
        let marker = match prompt.direction {
            crate::app::state::CopyModeSearchDirection::Forward => "/",
            crate::app::state::CopyModeSearchDirection::Backward => "?",
        };
        Line::from(vec![
            Span::styled(" COPY ", mode_style),
            Span::raw(" "),
            Span::styled(marker, key),
            Span::styled(prompt.query.clone(), Style::default().fg(app.palette.text)),
            Span::styled("█", key),
            Span::styled("  enter search  esc cancel", dim),
        ])
    } else {
        let select = if copy_mode.selection.is_some() {
            "selecting"
        } else {
            "select"
        };
        let match_status = copy_mode
            .search
            .current
            .map(|current| format!(" {}/{}", current + 1, copy_mode.search.matches.len()))
            .or_else(|| (!copy_mode.search.query.is_empty()).then(|| " 0/0".to_string()))
            .unwrap_or_default();
        let (exit_keys, exit_label) =
            if copy_mode.search.query.is_empty() && copy_mode.selection.is_none() {
                ("q/esc", " exit")
            } else {
                ("esc", " clear  q exit")
            };
        Line::from(vec![
            Span::styled(" COPY ", mode_style),
            Span::raw(" "),
            Span::styled("h/j/k/l w/b/e { }", key),
            Span::styled(" move  ", dim),
            Span::styled("/ ?", key),
            Span::styled(" search  ", dim),
            Span::styled("n/N", key),
            Span::styled(format!(" repeat{match_status}  "), dim),
            Span::styled("v/space", key),
            Span::styled(format!(" {select}  "), dim),
            Span::styled("y/enter", key),
            Span::styled(" copy  ", dim),
            Span::styled(exit_keys, key),
            Span::styled(exit_label, dim),
        ])
    };

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_navigate_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.accent)
        .add_modifier(Modifier::BOLD);

    let kb = &app.keybinds;
    let new_tab = prefix_rhs_label(&kb.new_tab);
    let split_vertical = prefix_rhs_label(&kb.split_vertical);
    let split_horizontal = prefix_rhs_label(&kb.split_horizontal);
    let close_pane = prefix_rhs_label(&kb.close_pane);
    let zoom = prefix_rhs_label(&kb.zoom);
    let resize = prefix_rhs_label(&kb.resize_mode);
    let help = prefix_rhs_label(&kb.help);
    let settings = prefix_rhs_label(&kb.settings);
    let goto = prefix_rhs_label(&kb.goto);
    let detach = prefix_rhs_label(&kb.detach);
    let workspace_nav = format!(
        "{} / {}",
        keybind_label(&kb.navigate.workspace_up),
        keybind_label(&kb.navigate.workspace_down)
    );
    let line = Line::from(vec![
        Span::styled(" NAVIGATE ", mode_style),
        Span::raw(" "),
        Span::styled("esc", key),
        Span::styled(" back  ", dim),
        Span::styled(workspace_nav, key),
        Span::styled(" ws  ", dim),
        Span::styled("⇥", key),
        Span::styled(" pane  ", dim),
        Span::styled(goto, key),
        Span::styled(" navigator  ", dim),
        Span::styled(new_tab, key),
        Span::styled(" new tab  ", dim),
        Span::styled(split_vertical, key),
        Span::styled(" split│  ", dim),
        Span::styled(split_horizontal, key),
        Span::styled(" split─  ", dim),
        Span::styled(close_pane, key),
        Span::styled(" close  ", dim),
        Span::styled(zoom, key),
        Span::styled(" zoom  ", dim),
        Span::styled(resize, key),
        Span::styled(" resize  ", dim),
        Span::styled(help, key),
        Span::styled(" keybinds  ", dim),
        Span::styled(settings, key),
        Span::styled(" settings  ", dim),
        Span::styled(detach, key),
        Span::styled(" detach", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);

    if app.update_available.is_some() {
        let status = Line::from(vec![Span::styled(
            " update ready",
            Style::default()
                .fg(app.palette.accent)
                .add_modifier(Modifier::BOLD),
        )]);
        let width = 13u16.min(overlay_area.width);
        let status_area = Rect::new(
            overlay_area.x + overlay_area.width.saturating_sub(width),
            overlay_area.y,
            width,
            overlay_area.height,
        );
        frame.render_widget(Clear, status_area);
        frame.render_widget(
            Paragraph::new(status).alignment(Alignment::Right),
            status_area,
        );
    }
}

pub(super) fn render_global_launcher_menu(app: &AppState, frame: &mut Frame) {
    let rect = app.global_menu_rect();
    let Some(inner) = render_panel_shell(frame, rect, app.palette.accent, app.palette.panel_bg)
    else {
        return;
    };

    let items = app.global_menu_labels();
    for (idx, item) in items.iter().enumerate() {
        let y = inner.y + idx as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let selected = idx == app.global_menu.highlighted;
        let rect = Rect::new(inner.x, y, inner.width, 1);

        let selected_style = Style::default()
            .fg(panel_contrast_fg(&app.palette))
            .bg(app.palette.accent)
            .add_modifier(Modifier::BOLD);
        let item_style = if selected {
            selected_style
        } else {
            Style::default().fg(app.palette.text)
        };
        let badge_style = if selected {
            selected_style
        } else {
            Style::default()
                .fg(app.palette.accent)
                .add_modifier(Modifier::BOLD)
        };

        let line = if app.global_menu_item_has_badge(item) {
            Line::from(vec![
                Span::styled(" ●", badge_style),
                Span::styled(format!(" {item} "), item_style),
            ])
        } else {
            Line::from(Span::styled(format!(" {item} "), item_style))
        };
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), rect);
    }
}

pub(super) fn render_resize_overlay(app: &AppState, frame: &mut Frame, area: Rect) {
    let key = Style::default()
        .fg(app.palette.accent)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(app.palette.overlay0);

    let mode_style = Style::default()
        .fg(panel_contrast_fg(&app.palette))
        .bg(app.palette.mauve)
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::styled(" RESIZE ", mode_style),
        Span::raw("  "),
        Span::styled("h/l", key),
        Span::styled(" width  ", dim),
        Span::styled("j/k", key),
        Span::styled(" height  ", dim),
        Span::styled("esc", key),
        Span::styled(" done", dim),
    ]);

    let overlay_y = area.y + area.height.saturating_sub(1);
    let overlay_area = Rect::new(area.x, overlay_y, area.width, 1);
    render_bottom_bar(frame, overlay_area, line, app.palette.panel_bg);
}

pub(super) fn render_context_menu(app: &AppState, frame: &mut Frame) {
    let Some(menu) = &app.context_menu else {
        return;
    };

    let p = &app.palette;
    let Some(menu_rect) = app.context_menu_rect() else {
        return;
    };
    // The polish pass: a quiet border (the accent belongs to the selection, not
    // the frame), and the selected row drawn the editor way - a subtle selection
    // background under the whole row with a thin accent bar at its left edge,
    // instead of a loud full-accent block.
    let Some(inner) = render_panel_shell(frame, menu_rect, p.surface1, p.panel_bg) else {
        return;
    };

    // The tag color picker prefixes each accent name with a swatch glyph in that
    // accent's own color, so the choice reads by color as well as by name.
    let swatch_color = |idx: usize| -> Option<ratatui::style::Color> {
        match &menu.kind {
            crate::app::state::ContextMenuKind::TagColor { .. } => crate::ui::TagAccent::ALL
                .get(idx)
                .map(|accent| accent.color(p)),
            _ => None,
        }
    };

    for (idx, item) in menu.items().iter().enumerate() {
        let y = inner.y + idx as u16;
        if y >= inner.y + inner.height {
            break;
        }
        let row = Rect::new(inner.x, y, inner.width, 1);
        let selected = idx == menu.list.highlighted;
        let swatch = swatch_color(idx);
        if selected {
            let row_bg = Style::default().bg(p.selection_bg);
            let buf = frame.buffer_mut();
            for x in row.x..row.x + row.width {
                buf[(x, y)].set_style(row_bg);
            }
            let mut spans = vec![Span::styled(
                "▎",
                Style::default().fg(p.accent).bg(p.selection_bg),
            )];
            if let Some(color) = swatch {
                spans.push(Span::styled(
                    "● ",
                    Style::default().fg(color).bg(p.selection_bg),
                ));
            }
            spans.push(Span::styled(
                format!("{item} "),
                Style::default()
                    .fg(p.text)
                    .bg(p.selection_bg)
                    .add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(Paragraph::new(Line::from(spans)), row);
        } else {
            let mut spans = vec![Span::raw(" ")];
            if let Some(color) = swatch {
                spans.push(Span::styled("● ", Style::default().fg(color)));
            }
            spans.push(Span::styled(
                format!("{item} "),
                Style::default().fg(p.text),
            ));
            frame.render_widget(Paragraph::new(Line::from(spans)), row);
        }
    }
}
