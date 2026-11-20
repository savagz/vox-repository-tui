use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::Row,
};

use crate::app::App;
use crate::i18n::Key;
use crate::theme::glyph;
use crate::ui::split_list_detail;
use crate::ui::tabs::Tab;
use crate::ui::widgets::list::{self, TableSpec, cell};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let (left, right) = split_list_detail(area, 50);
    let accent = theme.tab_accent(Tab::Remotes.index());
    let visible = repo.visible(Tab::Remotes);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let r = &repo.snap.remotes[i];
            let branches = repo.snap.branches.iter().filter(|b| b.is_remote && b.name.starts_with(&format!("{}/", r.name))).count();
            let url = if r.push_url != r.fetch_url && !r.push_url.is_empty() { format!("{} (push: {})", r.fetch_url, r.push_url) } else { r.fetch_url.clone() };
            Row::new(vec![
                cell(glyph::LED_ON, theme.style_for_color(accent)),
                cell(r.name.clone(), theme.style_for_bold()),
                cell(branches.to_string(), theme.readout_style()),
                cell(url, theme.value_style()),
            ])
        })
        .collect();
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabRemotes),
            accent,
            readout: Some(repo.snap.remotes.len().to_string()),
            header: vec!["", app.t(Key::ColName), app.t(Key::TabBranches), "URL"],
            widths: vec![Constraint::Length(1), Constraint::Length(14), Constraint::Length(5), Constraint::Fill(1)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[Tab::Remotes.index()]),
            focused: true,
            empty: app.t(Key::NoRemotes),
        },
    );
    list::detail(frame, right, app, app.t(Key::Detail), accent);
}
