use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::Row,
};

use crate::app::App;
use crate::config::shorten_home;
use crate::i18n::Key;
use crate::theme::glyph;
use crate::ui::split_list_detail;
use crate::ui::tabs::Tab;
use crate::ui::widgets::list::{self, TableSpec, cell};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let (left, right) = split_list_detail(area, 55);
    let accent = theme.tab_accent(Tab::Worktrees.index());
    let current = repo.ctx.root.display().to_string();
    let visible = repo.visible(Tab::Worktrees);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let w = &repo.snap.worktrees[i];
            let here = w.path == current;
            let (led, color) = if w.prunable {
                (glyph::LED_ON, theme.error)
            } else if here {
                (glyph::LED_ON, theme.accent_primary)
            } else {
                (glyph::LED_OFF, accent)
            };
            let mut flags = Vec::new();
            if w.bare {
                flags.push("bare");
            }
            if w.locked {
                flags.push("locked");
            }
            if w.prunable {
                flags.push("prunable");
            }
            let branch = w.branch.clone().unwrap_or_else(|| if w.detached { format!("({})", app.t(Key::Detached)) } else { String::new() });
            Row::new(vec![
                cell(led, theme.style_for_color(color)),
                cell(shorten_home(std::path::Path::new(&w.path)), if here { theme.style_for_bold() } else { theme.value_style() }),
                cell(branch, theme.style_for_color(theme.accent_quinary)),
                cell(w.head.clone(), theme.readout_style()),
                cell(flags.join(","), theme.style_for_color(theme.warning)),
            ])
        })
        .collect();
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabWorktrees),
            accent,
            readout: Some(repo.snap.worktrees.len().to_string()),
            header: vec!["", app.t(Key::ColPath), app.t(Key::ColBranch), "SHA", ""],
            widths: vec![Constraint::Length(1), Constraint::Fill(2), Constraint::Fill(1), Constraint::Length(7), Constraint::Length(9)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[Tab::Worktrees.index()]),
            focused: true,
            empty: app.t(Key::Loading),
        },
    );
    list::detail(frame, right, app, app.t(Key::Status), accent);
}
