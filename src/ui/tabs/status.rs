use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Paragraph, Row},
};

use crate::app::App;
use crate::app::conflicts::ConflictView;
use crate::app::staging::Row as DRow;
use crate::git::conflict::{self, Choice};
use crate::git::model::StatusEntry;
use crate::i18n::{self, Key};
use crate::theme::{Theme, glyph};
use crate::ui::widgets::list::{self, TableSpec, cell};
use crate::ui::widgets::panel;
use crate::ui::{format, split_list_detail};

fn entry_color(theme: &Theme, e: &StatusEntry) -> ratatui::style::Color {
    if e.is_conflicted() {
        theme.error
    } else if e.is_untracked() {
        theme.accent_tertiary
    } else if e.has_unstaged() {
        theme.warning
    } else {
        theme.ok
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let (left, right) = split_list_detail(area, 38);
    let visible = repo.visible(crate::ui::tabs::Tab::Status);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let e = &repo.snap.status.entries[i];
            let code: Vec<char> = e.code().chars().collect();
            let path = match &e.orig_path {
                Some(o) => format!("{o} → {}", e.path),
                None => e.path.clone(),
            };
            Row::new(vec![
                cell(glyph::LED_ON, theme.style_for_color(entry_color(theme, e))),
                ratatui::widgets::Cell::from(Line::from(vec![
                    Span::styled(code[0].to_string(), theme.style_for_color(if e.is_conflicted() { theme.error } else { theme.ok })),
                    Span::styled(code[1].to_string(), theme.style_for_color(if e.is_conflicted() { theme.error } else { theme.warning })),
                ])),
                cell(path, theme.value_style()),
            ])
        })
        .collect();
    let c = repo.snap.status.counts();
    let mut readout = i18n::fill(app.t(Key::StatusReadoutTpl), &[&c.staged.to_string(), &c.unstaged.to_string(), &c.untracked.to_string()]);
    if !repo.filter.value.is_empty() || repo.filter_editing {
        readout = format!("/{}{}", repo.filter.value, if repo.filter_editing { "▏" } else { "" });
    }
    let focus_list = !repo.diff_focus && repo.conflict.is_none();
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabStatus),
            accent: theme.tab_accent(2),
            readout: Some(readout),
            header: vec!["", "XY", app.t(Key::ColFile)],
            widths: vec![Constraint::Length(1), Constraint::Length(2), Constraint::Fill(1)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[crate::ui::tabs::Tab::Status.index()]),
            focused: focus_list,
            empty: if repo.loaded { app.t(Key::TreeClean) } else { app.t(Key::Loading) },
        },
    );

    if let Some(view) = &repo.conflict {
        render_conflict(frame, right, app, view);
    } else {
        render_diff(frame, right, app);
    }
}

fn render_diff(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let Some(st) = &repo.staging else {
        list::detail(frame, area, app, app.t(Key::Diff), theme.accent_secondary);
        return;
    };
    let side = if st.untracked {
        app.t(Key::Untracked)
    } else if st.staged {
        app.t(Key::Staged)
    } else {
        app.t(Key::Unstaged)
    };
    let title = format!("{} · {side}", app.t(Key::Diff));
    let hunk = st.hunk_at(st.cursor).map(|h| h + 1).unwrap_or(0);
    let mut readout = format!("HUNK {hunk}/{}", st.diff.hunks.len());
    if st.anchor.is_some() {
        readout.push_str(&format!("  {}", app.t(Key::RangeMode)));
    }
    if repo.detail.loading {
        readout = format!("{} {readout}", glyph::SPINNER[(app.tick % 4) as usize]);
    }
    let accent = if st.staged { theme.ok } else { theme.warning };
    let block = panel::block(theme, &title, accent, repo.diff_focus).title(panel::readout(theme, &readout, accent).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if st.diff.binary {
        frame.render_widget(Paragraph::new(Span::styled(app.t(Key::BinaryFile), theme.label_style())), inner);
        return;
    }
    if st.rows.is_empty() {
        frame.render_widget(Paragraph::new(Span::styled(app.t(Key::NoDetail), theme.label_style())), inner);
        return;
    }
    let h = inner.height as usize;
    let offset = if repo.diff_focus {
        st.cursor.saturating_sub(h / 2).min(st.rows.len().saturating_sub(h))
    } else {
        (repo.detail.scroll as usize).min(st.rows.len().saturating_sub(1))
    };
    let cur_hunk = st.hunk_at(st.cursor);
    let lines: Vec<Line> = st
        .rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(h)
        .map(|(i, row)| {
            let text = st.text(*row).replace('\t', "    ");
            let mut style = format::diff_style(theme, &text);
            let selected = repo.diff_focus && st.in_range(i) && st.is_change(i);
            if selected {
                style = style.bg(theme.bg_selected).add_modifier(Modifier::BOLD);
            }
            let in_hunk = repo.diff_focus && matches!(row, DRow::Hunk(hh) | DRow::Line(hh, _) if Some(*hh) == cur_hunk);
            let gutter = if selected {
                Span::styled("▌", theme.style_for_color(accent))
            } else if in_hunk {
                Span::styled("│", theme.style_for_color(theme.fg_dim))
            } else {
                Span::raw(" ")
            };
            Line::from(vec![gutter, Span::styled(text, style)])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_conflict(frame: &mut Frame, area: Rect, app: &App, view: &ConflictView) {
    let theme = &app.theme;
    let readout = i18n::fill(app.t(Key::ConflictReadoutTpl), &[&view.unresolved().to_string(), &view.count().to_string()]);
    let block = panel::block(theme, &format!("{} · {}", app.t(Key::Conflicts), view.path), theme.error, true)
        .title(panel::readout(theme, &readout, theme.error).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let mut sel_line = 0;
    for (i, b) in conflict::blocks(&view.segments).enumerate() {
        let selected = i == view.sel;
        if selected {
            sel_line = lines.len();
        }
        let (choice, color) = match b.choice {
            Choice::Unresolved => (app.t(Key::Unresolved), theme.error),
            Choice::Ours => (app.t(Key::Ours), theme.ok),
            Choice::Theirs => (app.t(Key::Theirs), theme.info),
            Choice::Both => (app.t(Key::Both), theme.accent_primary),
        };
        lines.push(Line::from(vec![
            Span::styled(if selected { "▌" } else { " " }, theme.style_for_color(theme.error)),
            Span::styled(format!(" #{} ", i + 1), theme.plate_style(color)),
            Span::styled(format!(" {choice}"), theme.style_for_color(color)),
        ]));
        let dim = |active: bool| if active || b.choice == Choice::Unresolved { 1.0 } else { 0.0 };
        let side = |label: &str, body: &[String], col: ratatui::style::Color, active: f32| -> Vec<Line<'static>> {
            let mut out = vec![Line::from(Span::styled(format!("   ── {label}"), theme.label_style()))];
            for l in body {
                let style = if active > 0.5 { theme.style_for_color(col) } else { theme.style_for_color(theme.fg_dim) };
                out.push(Line::from(Span::styled(format!("   {}", l.trim_end_matches(['\n', '\r']).replace('\t', "    ")), style)));
            }
            out
        };
        let ours_active = matches!(b.choice, Choice::Ours | Choice::Both);
        let theirs_active = matches!(b.choice, Choice::Theirs | Choice::Both);
        lines.extend(side(&format!("{} ({})", app.t(Key::Ours), b.ours_label), &b.ours, theme.ok, dim(ours_active)));
        lines.extend(side(&format!("{} ({})", app.t(Key::Theirs), b.theirs_label), &b.theirs, theme.info, dim(theirs_active)));
        lines.push(Line::from(""));
    }
    let h = inner.height as usize;
    let offset = sel_line.saturating_sub(1).min(lines.len().saturating_sub(h));
    frame.render_widget(Paragraph::new(lines.into_iter().skip(offset).take(h).collect::<Vec<_>>()), inner);
}
