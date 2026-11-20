use chrono::Local;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::panel;
use crate::app::App;
use crate::i18n::{self, Key};
use crate::theme::{Status, glyph};
use crate::ui::format;

/// Franja superior: identificación del repo (o del workspace) a la izquierda,
/// pilotos de estado a la derecha, como el frente de un rack.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = panel::block(theme, app.t(Key::HeaderTitle), theme.accent_primary, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(inner);

    let left = match &app.repo {
        Some(repo) => {
            let head = &repo.snap.status.head;
            let branch = head
                .head
                .clone()
                .unwrap_or_else(|| format!("({} {})", app.t(Key::Detached), head.oid.as_deref().map(|o| &o[..o.len().min(7)]).unwrap_or("")));
            let mut spans = vec![
                Span::styled("REPO ", theme.label_style()),
                Span::styled(repo.ctx.name.clone(), theme.readout_style()),
                Span::styled(format!("  {} ", glyph::BRANCH), theme.style_for_color(theme.accent_quinary)),
                Span::styled(branch, theme.style_for_bold()),
            ];
            if let Some(up) = &head.upstream {
                spans.push(Span::styled(format!(" → {up}"), theme.label_style()));
            }
            Line::from(spans)
        }
        None => Line::from(vec![
            Span::styled("WORKSPACE ", theme.label_style()),
            Span::styled(format!("{} REPOS", app.ws.repos.len()), theme.readout_style()),
        ]),
    };
    frame.render_widget(Paragraph::new(left), cols[0]);

    let mut pilots: Vec<Span> = Vec::new();
    if app.busy > 0 || app.opening || app.ws.scanning || app.ws.pending > 0 {
        let spin = glyph::SPINNER[(app.tick % 4) as usize];
        let what = if app.busy > 0 { format::truncate(&app.busy_desc, 24) } else { app.t(Key::Working).to_string() };
        pilots.push(Span::styled(format!("{spin} {what}  "), theme.style_for(Status::Warning)));
    }
    if let Some(repo) = &app.repo {
        let head = &repo.snap.status.head;
        let c = repo.snap.status.counts();
        if c.conflicted > 0 {
            pilots.push(Span::styled(format!("{} {} CONFL  ", glyph::LED_ON, c.conflicted), theme.style_for(Status::Error)));
        }
        let (led, st) = if c.is_clean() { (glyph::LED_ON, Status::Ok) } else { (glyph::LED_ON, Status::Warning) };
        let tree = if c.is_clean() { app.t(Key::Clean).to_string() } else { i18n::fill(app.t(Key::ChangesTpl), &[&c.total().to_string()]) };
        pilots.push(Span::styled(format!("{led} {tree}  "), theme.style_for(st)));
        if head.upstream.is_some() {
            let sync_style = if head.ahead + head.behind == 0 { theme.label_style() } else { theme.readout_style() };
            pilots.push(Span::styled(format!("{}{} {}{}  ", glyph::UP, head.ahead, glyph::DOWN, head.behind), sync_style));
        }
    } else {
        let dirty = app.ws.dirty_count();
        let st = if dirty == 0 { Status::Ok } else { Status::Warning };
        pilots.push(Span::styled(format!("{} {}  ", glyph::LED_ON, i18n::fill(app.t(Key::WsDirtyTpl), &[&dirty.to_string()])), theme.style_for(st)));
    }
    pilots.push(Span::styled(Local::now().format("%H:%M:%S").to_string(), theme.readout_style()));
    frame.render_widget(Paragraph::new(Line::from(pilots)).alignment(Alignment::Right), cols[1]);
}
