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
    let (left, right) = split_list_detail(area, 40);
    let accent = theme.tab_accent(Tab::Stash.index());
    let now = format::now();
    let visible = repo.visible(Tab::Stash);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let s = &repo.snap.stashes[i];
            Row::new(vec![
                cell(s.selector.trim_start_matches("stash@").to_string(), theme.readout_style()),
                cell(format::ago(app.lang, s.time, now), theme.label_style()),
                cell(s.message.clone(), theme.value_style()),
            ])
        })
        .collect();
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabStash),
            accent,
            readout: Some(repo.snap.stashes.len().to_string()),
            header: vec!["#", app.t(Key::ColAge), app.t(Key::ColMessage)],
            widths: vec![Constraint::Length(5), Constraint::Length(4), Constraint::Fill(1)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[Tab::Stash.index()]),
            focused: true,
            empty: app.t(Key::NoStash),
        },
    );
    list::detail(frame, right, app, app.t(Key::Diff), accent);
}
