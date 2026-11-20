use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

use super::panel;
use crate::app::modal::{CommitForm, Confirm, Form, Menu, Output, TextInput};
use crate::app::planner::Planner;
use crate::config::{ConfigField, ConfigForm};
use crate::git::rebase::{Action, PlanError};
use crate::i18n::{self, Key, Lang};
use crate::theme::{Status, Theme, hazard_stripe};

fn keys_line<'a>(theme: &Theme, items: &[(&str, &'a str, Color)]) -> Line<'a> {
    let mut spans = Vec::new();
    for (k, label, color) in items {
        spans.push(Span::styled(format!(" {k} "), theme.plate_style(*color)));
        spans.push(Span::styled(format!(" {label}  "), theme.label_style()));
    }
    Line::from(spans)
}

/// Campo con cursor visible.
fn input_spans<'a>(theme: &Theme, input: &TextInput, focused: bool, accent: Color) -> Vec<Span<'a>> {
    if !focused {
        return vec![Span::styled(input.value.clone(), theme.value_style())];
    }
    let (before, at, after) = input.split();
    vec![
        Span::styled(before, theme.selected_row_style()),
        Span::styled(at, theme.plate_style(accent).add_modifier(ratatui::style::Modifier::REVERSED)),
        Span::styled(after, theme.selected_row_style()),
    ]
}

fn open(frame: &mut Frame, area: Rect, theme: &Theme, title: &str, accent: Color, w: u16, h: u16) -> Rect {
    open_rect(frame, panel::centered(area, w, h), theme, title, accent)
}

fn open_rows(frame: &mut Frame, area: Rect, theme: &Theme, title: &str, accent: Color, w: u16, rows: u16) -> Rect {
    open_rect(frame, panel::centered_rows(area, w, rows), theme, title, accent)
}

fn open_rect(frame: &mut Frame, modal: Rect, theme: &Theme, title: &str, accent: Color) -> Rect {
    frame.render_widget(Clear, modal);
    let block = panel::block(theme, title, accent, true);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    inner
}

pub fn form(frame: &mut Frame, area: Rect, f: &Form, theme: &Theme, lang: Lang) {
    let accent = theme.accent_primary;
    // Borde y margen (4) + subtítulo + campos + error + hueco + teclas.
    let rows = 4 + if f.subtitle.is_empty() { 0 } else { 2 } + f.fields.len() as u16 + 4;
    let inner = open_rows(frame, area, theme, &f.title, accent, 70, rows);
    let mut constraints = vec![Constraint::Length(if f.subtitle.is_empty() { 0 } else { 2 })];
    constraints.extend(f.fields.iter().map(|_| Constraint::Length(1)));
    constraints.extend([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]);
    let rows = Layout::default().direction(Direction::Vertical).margin(1).constraints(constraints).split(inner);
    frame.render_widget(Paragraph::new(f.subtitle.clone()).style(theme.label_style()).wrap(Wrap { trim: true }), rows[0]);
    let label_w = f.fields.iter().map(|x| x.label.chars().count()).max().unwrap_or(8).max(8) + 1;
    for (i, field) in f.fields.iter().enumerate() {
        let focused = f.focus == i;
        let mut spans = vec![
            Span::styled(if focused { "▌" } else { " " }, theme.style_for_color(accent)),
            Span::styled(format!("{:<label_w$}", field.label), if focused { theme.style_for_bold() } else { theme.label_style() }),
            Span::styled("│ ", theme.style_for_color(theme.border)),
        ];
        if field.choices.is_some() {
            spans.push(Span::styled(field.input.value.clone(), if focused { theme.selected_row_style() } else { theme.value_style() }));
            if focused {
                spans.push(Span::styled(" ◀ ▶", theme.style_for_color(accent)));
            }
        } else {
            spans.extend(input_spans(theme, &field.input, focused, accent));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), rows[i + 1]);
    }
    let n = f.fields.len();
    if let Some(e) = &f.error {
        frame.render_widget(Paragraph::new(Span::styled(e.clone(), theme.style_for(Status::Error))), rows[n + 2]);
    }
    let t = |k| i18n::t(lang, k);
    frame.render_widget(
        Paragraph::new(keys_line(theme, &[("TAB", t(Key::NextField), theme.accent_secondary), ("ENTER", t(Key::Confirm), theme.ok), ("ESC", t(Key::Cancel), theme.accent_tertiary)])),
        rows[n + 3],
    );
}

pub fn confirm(frame: &mut Frame, area: Rect, c: &Confirm, theme: &Theme, lang: Lang) {
    let accent = if c.typed.is_some() { theme.error } else { theme.warning };
    let msg_lines = (c.message.chars().count() as u16 / (area.width * 64 / 100).saturating_sub(4).max(10)) + 1;
    let rows = 4 + 1 + msg_lines.max(2) + if c.typed.is_some() { 2 } else { 0 } + 2;
    let inner = open_rows(frame, area, theme, &c.title, accent, 64, rows);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Min(2), Constraint::Length(if c.typed.is_some() { 2 } else { 0 }), Constraint::Length(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(Span::styled(hazard_stripe(rows[0].width.min(64) as usize), theme.style_for_color(accent))),
        rows[0],
    );
    frame.render_widget(Paragraph::new(c.message.clone()).style(theme.value_style()).wrap(Wrap { trim: true }), rows[1]);
    let t = |k| i18n::t(lang, k);
    if let Some(expected) = &c.typed {
        let style = if c.ready() { theme.style_for(Status::Ok) } else { theme.selected_row_style() };
        let lines = vec![
            Line::from(vec![Span::styled(t(Key::TypeToConfirm), theme.label_style()), Span::styled(expected.clone(), theme.readout_style())]),
            Line::from(vec![
                Span::styled("▌ ", theme.style_for_color(accent)),
                Span::styled(c.input.value.clone(), style),
                Span::styled("▏", theme.style_for_color(accent)),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), rows[2]);
        frame.render_widget(Paragraph::new(keys_line(theme, &[("ENTER", t(Key::Confirm), accent), ("ESC", t(Key::Cancel), theme.accent_tertiary)])), rows[3]);
    } else {
        frame.render_widget(
            Paragraph::new(keys_line(theme, &[("Y/ENTER", t(Key::Confirm), accent), ("N/ESC", t(Key::Cancel), theme.accent_tertiary)])),
            rows[3],
        );
    }
}

pub fn menu(frame: &mut Frame, area: Rect, m: &Menu, theme: &Theme, lang: Lang) {
    let accent = theme.accent_secondary;
    // Borde (2) + margen (2) + opciones + línea de teclas.
    let modal = panel::centered_rows(area, 50, m.items.len() as u16 + 5);
    frame.render_widget(Clear, modal);
    let block = panel::block(theme, &m.title, accent, true);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let lines: Vec<Line> = m
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let sel = i == m.sel;
            Line::from(vec![
                Span::styled(if sel { "▌" } else { " " }, theme.style_for_color(accent)),
                Span::styled(format!(" {} ", item.key.to_ascii_uppercase()), theme.plate_style(accent)),
                Span::styled(format!(" {}", item.label), if sel { theme.selected_row_style() } else { theme.value_style() }),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[0]);
    let t = |k| i18n::t(lang, k);
    frame.render_widget(Paragraph::new(keys_line(theme, &[("ENTER", t(Key::Confirm), theme.ok), ("ESC", t(Key::Cancel), theme.accent_tertiary)])), rows[1]);
}

pub fn output(frame: &mut Frame, area: Rect, o: &Output, theme: &Theme, lang: Lang) {
    let accent = if o.error { theme.error } else { theme.accent_secondary };
    let modal = panel::centered(area, 84, 80);
    frame.render_widget(Clear, modal);
    let readout = format!("{}/{}", o.scroll as usize + 1, o.lines.len());
    let block = panel::block(theme, &o.title, accent, true)
        .title(panel::readout(theme, &readout, theme.fg_muted).right_aligned())
        .title_bottom(panel::readout(theme, i18n::t(lang, Key::AnyKeyClose), theme.fg_muted));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);
    let lines: Vec<Line> = o
        .lines
        .iter()
        .skip(o.scroll as usize)
        .take(inner.height as usize)
        .map(|l| {
            if o.error {
                Line::from(Span::styled(l.clone(), theme.style_for_color(theme.error)))
            } else {
                crate::ui::format::diff_line(theme, l)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

pub fn commit(frame: &mut Frame, area: Rect, c: &CommitForm, theme: &Theme, lang: Lang) {
    let accent = if c.amend { theme.warning } else { theme.accent_primary };
    let title = i18n::t(lang, if c.amend { Key::FormAmend } else { Key::FormCommit });
    let inner = open(frame, area, theme, title, accent, 74, 60);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);
    let t = |k| i18n::t(lang, k);
    let len = c.summary.value.chars().count();
    let len_style = if len > 72 { theme.style_for(Status::Error) } else if len > 50 { theme.style_for(Status::Warning) } else { theme.label_style() };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(t(Key::FieldSummary), theme.style_for_bold()),
            Span::styled(format!("  {len}/50"), len_style),
        ])),
        rows[0],
    );
    let mut spans = vec![Span::styled(if c.focus == 0 { "▌" } else { " " }, theme.style_for_color(accent))];
    spans.extend(input_spans(theme, &c.summary, c.focus == 0, accent));
    frame.render_widget(Paragraph::new(Line::from(spans)), rows[1]);
    frame.render_widget(Paragraph::new(Span::styled(t(Key::FieldBody), theme.style_for_bold())), rows[2]);

    let body_h = rows[3].height as usize;
    let cur = c.focus.saturating_sub(1);
    let start = cur.saturating_sub(body_h.saturating_sub(1));
    let body: Vec<Line> = c
        .body
        .iter()
        .enumerate()
        .skip(start)
        .take(body_h)
        .map(|(i, l)| {
            let focused = c.focus == i + 1;
            let mut spans = vec![Span::styled(if focused { "▌" } else { "│" }, theme.style_for_color(if focused { accent } else { theme.border }))];
            spans.extend(input_spans(theme, l, focused, accent));
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(body), rows[3]);
    if let Some(e) = &c.error {
        frame.render_widget(Paragraph::new(Span::styled(e.clone(), theme.style_for(Status::Error))), rows[4]);
    }
    frame.render_widget(
        Paragraph::new(keys_line(
            theme,
            &[
                ("TAB", t(Key::NextField), theme.accent_secondary),
                ("ENTER", t(Key::CommitEnterHint), theme.ok),
                ("^S", t(Key::Confirm), theme.ok),
                ("ESC", t(Key::Cancel), theme.accent_tertiary),
            ],
        )),
        rows[5],
    );
}

fn action_color(theme: &Theme, a: Action) -> Color {
    match a {
        Action::Pick => theme.ok,
        Action::Reword => theme.accent_secondary,
        Action::Edit => theme.accent_quinary,
        Action::Squash | Action::Fixup => theme.warning,
        Action::Drop => theme.error,
    }
}

pub fn planner(frame: &mut Frame, area: Rect, p: &Planner, theme: &Theme, lang: Lang) {
    let accent = theme.warning;
    let inner = open(frame, area, theme, i18n::t(lang, Key::PlannerTitle), accent, 82, 76);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(inner);
    let base = p.base.as_deref().map(|b| b.chars().take(8).collect::<String>()).unwrap_or_else(|| "--root".into());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(hazard_stripe(4), theme.style_for_color(accent)),
            Span::styled(format!(" {} ", i18n::fill(i18n::t(lang, Key::PlannerHeaderTpl), &[&base])), theme.label_style()),
        ])),
        rows[0],
    );
    let h = rows[1].height as usize;
    let start = p.sel.saturating_sub(h.saturating_sub(1));
    let lines: Vec<Line> = p
        .items
        .iter()
        .enumerate()
        .skip(start)
        .take(h)
        .map(|(i, item)| {
            let sel = i == p.sel;
            let color = action_color(theme, item.action);
            let subject = item.message.as_deref().and_then(|m| m.lines().next()).unwrap_or(&item.subject);
            let subject_style = if item.action == Action::Drop {
                theme.style_for_color(theme.fg_dim).add_modifier(ratatui::style::Modifier::CROSSED_OUT)
            } else if sel {
                theme.selected_row_style()
            } else {
                theme.value_style()
            };
            Line::from(vec![
                Span::styled(if sel { "▌" } else { " " }, theme.style_for_color(accent)),
                Span::styled(format!(" {:<7}", item.action.word().to_uppercase()), theme.plate_style(color)),
                Span::styled(format!(" {} ", &item.hash[..item.hash.len().min(8)]), theme.readout_style()),
                Span::styled(subject.to_string(), subject_style),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[1]);
    if let Some(e) = p.error {
        let msg = match e {
            PlanError::Empty => i18n::t(lang, Key::PlanEmpty),
            PlanError::MeldFirst => i18n::t(lang, Key::PlanMeldFirst),
        };
        frame.render_widget(Paragraph::new(Span::styled(msg, theme.style_for(Status::Error))), rows[2]);
    } else {
        frame.render_widget(Paragraph::new(Span::styled(i18n::t(lang, Key::PlannerOrder), theme.label_style())), rows[2]);
    }
    let t = |k| i18n::t(lang, k);
    frame.render_widget(
        Paragraph::new(keys_line(
            theme,
            &[
                ("P R E S F D", t(Key::PlannerActionsShort), theme.warning),
                ("⇧J/⇧K", t(Key::HbMove), theme.accent_secondary),
                ("ENTER", t(Key::HelpPlannerRun), theme.ok),
                ("ESC", t(Key::Cancel), theme.accent_tertiary),
            ],
        )),
        rows[3],
    );
}

pub fn config(frame: &mut Frame, area: Rect, f: &ConfigForm, theme: &Theme) {
    let lang = f.language;
    let accent = theme.accent_primary;
    let inner = open(frame, area, theme, i18n::t(lang, Key::ConfigTitle), accent, 72, 56);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(ConfigField::ALL.len() as u16), Constraint::Length(2), Constraint::Length(2), Constraint::Length(1)])
        .split(inner);
    let lines: Vec<Line> = ConfigField::ALL
        .iter()
        .map(|&field| {
            let focused = f.field() == field;
            let mut spans = vec![
                Span::styled(if focused { "▌" } else { " " }, theme.style_for_color(accent)),
                Span::styled(format!("{:<22}", field.label(lang)), if focused { theme.style_for_bold() } else { theme.label_style() }),
                Span::styled("│ ", theme.style_for_color(theme.border)),
                Span::styled(f.value_of(field), if focused { theme.selected_row_style() } else { theme.value_style() }),
            ];
            if focused {
                spans.push(Span::styled(if field.is_choice() { " ◀ ▶" } else { "▏" }, theme.style_for_color(accent)));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows[0]);
    frame.render_widget(Paragraph::new(f.field().hint(lang)).style(theme.label_style()).wrap(Wrap { trim: true }), rows[1]);
    if let Some(e) = &f.error {
        frame.render_widget(Paragraph::new(e.clone()).style(theme.style_for(Status::Error)), rows[2]);
    }
    let t = |k| i18n::t(lang, k);
    frame.render_widget(
        Paragraph::new(keys_line(
            theme,
            &[("↑↓", t(Key::NextField), theme.accent_secondary), ("ENTER", t(Key::Save), theme.ok), ("ESC", t(Key::Cancel), theme.error)],
        )),
        rows[3],
    );
}
