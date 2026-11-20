use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Row},
};

use crate::app::App;
use crate::app::workspace::WsRepo;
use crate::config::shorten_home;
use crate::git::model::OpState;
use crate::i18n::{self, Key};
use crate::theme::{Theme, glyph};
use crate::ui::format;
use crate::ui::widgets::list::{self, TableSpec, cell};
use crate::ui::widgets::panel;

pub fn led(theme: &Theme, r: &WsRepo) -> (String, ratatui::style::Color) {
    if r.busy {
        return (glyph::LED_ON.into(), theme.info);
    }
    if r.error.is_some() {
        return (glyph::LED_ON.into(), theme.error);
    }
    let Some(s) = &r.summary else { return (glyph::LED_OFF.into(), theme.fg_dim) };
    if s.counts.conflicted > 0 || s.op != OpState::None {
        (glyph::LED_ON.into(), theme.error)
    } else if !s.counts.is_clean() {
        (glyph::LED_ON.into(), theme.warning)
    } else if s.head.behind > 0 || s.head.ahead > 0 {
        (glyph::LED_ON.into(), theme.accent_secondary)
    } else {
        (glyph::LED_ON.into(), theme.ok)
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let ws = &app.ws;
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(7)])
        .split(area);

    let now = format::now();
    let visible = ws.visible();
    let rows: Vec<Row> = visible
        .iter()
        .map(|&i| {
            let r = &ws.repos[i];
            let (g, color) = led(theme, r);
            let (branch, sync, changes, stash, last, subject) = match &r.summary {
                Some(s) => {
                    let branch = s.head.head.clone().unwrap_or_else(|| format!("({})", app.t(Key::Detached)));
                    let sync = if s.head.upstream.is_none() {
                        "—".to_string()
                    } else if s.head.ahead + s.head.behind == 0 {
                        "=".to_string()
                    } else {
                        format!("{}{} {}{}", glyph::UP, s.head.ahead, glyph::DOWN, s.head.behind)
                    };
                    let c = &s.counts;
                    let changes = if c.is_clean() {
                        app.t(Key::Clean).to_string()
                    } else {
                        let mut parts = vec![];
                        if c.staged > 0 {
                            parts.push(format!("+{}", c.staged));
                        }
                        if c.unstaged > 0 {
                            parts.push(format!("~{}", c.unstaged));
                        }
                        if c.untracked > 0 {
                            parts.push(format!("?{}", c.untracked));
                        }
                        if c.conflicted > 0 {
                            parts.push(format!("!{}", c.conflicted));
                        }
                        parts.join(" ")
                    };
                    let stash = if s.stashes > 0 { s.stashes.to_string() } else { String::new() };
                    let last = s.last_time.map(|t| format::ago(app.lang, t, now)).unwrap_or_else(|| "—".into());
                    let mut subject = s.last_subject.clone();
                    if s.op != OpState::None {
                        subject = format!("{} {}", s.op.label(), subject);
                    }
                    (branch, sync, changes, stash, last, subject)
                }
                None => {
                    let msg = r.error.clone().unwrap_or_else(|| "…".into());
                    (String::new(), String::new(), String::new(), String::new(), String::new(), msg)
                }
            };
            let changes_style = if changes == app.t(Key::Clean) { theme.label_style() } else { theme.style_for_color(theme.warning) };
            Row::new(vec![
                cell(g, theme.style_for_color(color)),
                cell(r.name.clone(), theme.style_for_bold()),
                cell(branch, theme.style_for_color(theme.accent_quinary)),
                cell(sync, theme.readout_style()),
                cell(changes, changes_style),
                cell(stash, theme.style_for_color(theme.accent_tertiary)),
                cell(last, theme.label_style()),
                cell(subject, if r.error.is_some() { theme.style_for_color(theme.error) } else { theme.value_style() }),
            ])
        })
        .collect();

    let mut readout = format!("{}/{}  {}", visible.len(), ws.repos.len(), app.t(ws.sort.key()));
    if ws.only_dirty {
        readout.push_str(&format!("  {}", app.t(Key::OnlyDirtyShort)));
    }
    if !ws.filter.value.is_empty() || ws.filter_editing {
        readout.push_str(&format!("  /{}{}", ws.filter.value, if ws.filter_editing { "▏" } else { "" }));
    }
    if ws.scanning {
        readout = format!("{} {}", glyph::SPINNER[(app.tick % 4) as usize], app.t(Key::Scanning));
    }
    let empty = if ws.scanning { app.t(Key::Scanning) } else { app.t(Key::WsEmpty) };
    list::table(
        frame,
        parts[0],
        theme,
        TableSpec {
            title: app.t(Key::TabWorkspace),
            accent: theme.tab_accent(0),
            readout: Some(readout),
            header: vec!["", app.t(Key::ColRepo), app.t(Key::ColBranch), "SYNC", app.t(Key::ColChanges), "ST", app.t(Key::ColAge), app.t(Key::ColLastCommit)],
            widths: vec![
                Constraint::Length(1),
                Constraint::Percentage(22),
                Constraint::Percentage(14),
                Constraint::Length(9),
                Constraint::Length(14),
                Constraint::Length(2),
                Constraint::Length(4),
                Constraint::Fill(1),
            ],
            rows,
            selected: (!visible.is_empty()).then_some(ws.sel),
            focused: true,
            empty,
        },
    );

    render_detail(frame, parts[1], app);
}

fn render_detail(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel::block(theme, app.t(Key::Detail), theme.border, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(r) = app.ws.selected() else {
        let roots = app.config.roots().iter().map(|p| shorten_home(p)).collect::<Vec<_>>().join(", ");
        frame.render_widget(Paragraph::new(Span::styled(i18n::fill(app.t(Key::WsRootsTpl), &[&roots]), theme.label_style())), inner);
        return;
    };
    let label = |k: Key| Span::styled(format!("{:<10}", app.t(k)), theme.label_style());
    let mut lines = vec![Line::from(vec![label(Key::ColPath), Span::styled(shorten_home(&r.path), theme.readout_style())])];
    if let Some(s) = &r.summary {
        lines.push(Line::from(vec![
            label(Key::ColRemote),
            Span::styled(s.origin.clone().unwrap_or_else(|| "—".into()), theme.value_style()),
        ]));
        lines.push(Line::from(vec![
            label(Key::ColUpstream),
            Span::styled(s.head.upstream.clone().unwrap_or_else(|| "—".into()), theme.value_style()),
        ]));
        let c = &s.counts;
        lines.push(Line::from(vec![
            label(Key::ColChanges),
            Span::styled(
                i18n::fill(app.t(Key::CountsTpl), &[&c.staged.to_string(), &c.unstaged.to_string(), &c.untracked.to_string(), &c.conflicted.to_string()]),
                theme.value_style(),
            ),
        ]));
    }
    if let Some(e) = &r.error {
        lines.push(Line::from(Span::styled(e.clone(), theme.style_for_color(theme.error))));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
