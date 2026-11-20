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
    let (left, right) = split_list_detail(area, 45);
    let accent = theme.tab_accent(Tab::Tags.index());
    let now = format::now();
    let visible = repo.visible(Tab::Tags);
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let t = &repo.snap.tags[i];
            Row::new(vec![
                cell(t.name.clone(), theme.style_for_bold()),
                cell(if t.annotated { "A" } else { "L" }, theme.label_style()),
                cell(t.short.clone(), theme.readout_style()),
                cell(format::ago(app.lang, t.time, now), theme.label_style()),
                cell(t.subject.clone(), theme.value_style()),
            ])
        })
        .collect();
    let mut readout = repo.snap.tags.len().to_string();
    if !repo.filter.value.is_empty() || repo.filter_editing {
        readout = format!("/{}{}", repo.filter.value, if repo.filter_editing { "▏" } else { "" });
    }
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: app.t(Key::TabTags),
            accent,
            readout: Some(readout),
            header: vec![app.t(Key::ColName), "", "SHA", app.t(Key::ColAge), app.t(Key::ColSubject)],
            widths: vec![Constraint::Percentage(30), Constraint::Length(1), Constraint::Length(8), Constraint::Length(4), Constraint::Fill(1)],
            rows,
            selected: (!visible.is_empty()).then_some(repo.sel[Tab::Tags.index()]),
            focused: true,
            empty: app.t(Key::NoTags),
        },
    );
    list::detail(frame, right, app, app.t(Key::Detail), accent);
}
