use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::Row,
};

use crate::app::App;
use crate::i18n::Key;
use crate::theme::glyph;
use crate::ui::tabs::Tab;
use crate::ui::widgets::list::{self, TableSpec, cell};
use crate::ui::{format, split_list_detail};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let (left, right) = split_list_detail(area, 50);
    let accent = theme.tab_accent(Tab::Branches.index());
    let now = format::now();
    let visible = repo.visible(Tab::Branches);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let b = &repo.snap.branches[i];
            let (led, color) = if b.is_head {
                (glyph::LED_ON, theme.accent_primary)
            } else if b.is_remote {
                (glyph::LED_OFF, theme.accent_senary)
            } else {
                (glyph::LED_ON, theme.accent_quinary)
            };
            let sync = if b.gone {
                app.t(Key::Gone).to_string()
            } else if b.upstream.is_some() {
                if b.ahead + b.behind == 0 { "=".into() } else { format!("{}{} {}{}", glyph::UP, b.ahead, glyph::DOWN, b.behind) }
            } else {
                String::new()
            };
            let name_style = if b.is_head { theme.style_for_bold() } else if b.is_remote { theme.label_style() } else { theme.value_style() };
            Row::new(vec![
                cell(led, theme.style_for_color(color)),
                cell(b.name.clone(), name_style),
                cell(sync, if b.gone { theme.style_for_color(theme.error) } else { theme.readout_style() }),
                cell(format::ago(app.lang, b.time, now), theme.label_style()),
                cell(b.subject.clone(), theme.label_style()),
            ])
        })
        .collect();
    let local = repo.snap.branches.iter().filter(|b| !b.is_remote).count();
    let mut readout = format!("{local} + {}", repo.snap.branches.len() - local);
    if !repo.filter.value.is_empty() || repo.filter_editing {
        readout = format!("/{}{}", repo.filter.value, if repo.filter_editing { "▏" } else { "" });
    }
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabBranches),
            accent,
            readout: Some(readout),
            header: vec!["", app.t(Key::ColBranch), "SYNC", app.t(Key::ColAge), app.t(Key::ColSubject)],
            widths: vec![Constraint::Length(1), Constraint::Percentage(40), Constraint::Length(9), Constraint::Length(4), Constraint::Fill(1)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[Tab::Branches.index()]),
            focused: true,
            empty: app.t(Key::NoBranches),
        },
    );
    list::detail(frame, right, app, app.t(Key::BranchLog), accent);
}
