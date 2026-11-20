use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::Row,
};

use crate::app::App;
use crate::i18n::Key;
use crate::ui::tabs::Tab;
use crate::ui::widgets::list::{self, TableSpec, cell};
use crate::ui::{format, split_list_detail};

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let (left, right) = split_list_detail(area, 50);
    let accent = theme.tab_accent(Tab::Reflog.index());
    let now = format::now();
    let visible = repo.visible(Tab::Reflog);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let r = &repo.snap.reflog[i];
            let (action, rest) = r.subject.split_once(": ").unwrap_or(("", &r.subject));
            let color = match action.split_whitespace().next().unwrap_or("") {
                "commit" => theme.ok,
                "checkout" => theme.accent_quinary,
                "reset" => theme.error,
                "rebase" | "rebase(finish)" | "rebase(start)" | "rebase(pick)" => theme.warning,
                "merge" | "pull" => theme.accent_secondary,
                _ => theme.fg_muted,
            };
            Row::new(vec![
                cell(r.short.clone(), theme.readout_style()),
                cell(r.selector.trim_start_matches("HEAD").to_string(), theme.label_style()),
                cell(format::ago(app.lang, r.time, now), theme.label_style()),
                cell(action.to_string(), theme.style_for_color(color)),
                cell(rest.to_string(), theme.value_style()),
            ])
        })
        .collect();
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabReflog),
            accent,
            readout: Some(repo.snap.reflog.len().to_string()),
            header: vec!["SHA", "#", app.t(Key::ColAge), app.t(Key::ColAction), app.t(Key::ColMessage)],
            widths: vec![Constraint::Length(8), Constraint::Length(6), Constraint::Length(4), Constraint::Length(14), Constraint::Fill(1)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[Tab::Reflog.index()]),
            focused: true,
            empty: app.t(Key::Loading),
        },
    );
    list::detail(frame, right, app, app.t(Key::Detail), accent);
}
