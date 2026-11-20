use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::app::App;
use crate::config::shorten_home;
use crate::i18n::Key;
use crate::theme::glyph;
use crate::ui::format;
use crate::ui::tabs::Tab;

/// Columna izquierda: botonera del tablero. Cada sección es un pulsador con
/// su LED de identidad; el activo queda "hundido". Una alarma (conflictos,
/// git ausente, repo con error) pisa la identidad y pone el LED en rojo. Las
/// secciones que necesitan repo quedan apagadas mientras no haya uno abierto.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let n = Tab::ALL.len() as u16;
    let compact = area.height < n * 3 + 4;
    let button_h = if compact { 1 } else { 3 };

    let mut constraints: Vec<Constraint> = Tab::ALL.iter().map(|_| Constraint::Length(button_h)).collect();
    constraints.push(Constraint::Min(0));
    let rows = Layout::default().direction(Direction::Vertical).constraints(constraints).split(area);

    for tab in Tab::ALL {
        render_button(frame, rows[tab.index()], tab, app, compact);
    }
    render_legend(frame, rows[Tab::ALL.len()], app);
}

fn has_alarm(tab: Tab, app: &App) -> bool {
    match tab {
        Tab::Workspace => app.ws.repos.iter().any(|r| r.error.is_some()),
        Tab::Status => app.repo.as_ref().is_some_and(|r| r.snap.status.counts().conflicted > 0),
        Tab::SystemCheck => !app.system_check.report.all_blocking_ok(),
        _ => false,
    }
}

fn is_live(tab: Tab, app: &App) -> bool {
    let Some(repo) = &app.repo else {
        return matches!(tab, Tab::Workspace | Tab::SystemCheck) && (tab == Tab::SystemCheck || !app.ws.repos.is_empty());
    };
    let s = &repo.snap;
    match tab {
        Tab::Workspace => !app.ws.repos.is_empty(),
        Tab::Overview | Tab::SystemCheck | Tab::Commits | Tab::Branches | Tab::Reflog => true,
        Tab::Status => !s.status.entries.is_empty(),
        Tab::Remotes => !s.remotes.is_empty(),
        Tab::Stash => !s.stashes.is_empty(),
        Tab::Tags => !s.tags.is_empty(),
        Tab::Worktrees => s.worktrees.len() > 1,
    }
}

/// Contador corto a la derecha del botón.
fn badge(tab: Tab, app: &App) -> Option<String> {
    let s = &app.repo.as_ref()?.snap;
    let n = match tab {
        Tab::Status => s.status.entries.len(),
        Tab::Branches => s.branches.iter().filter(|b| !b.is_remote).count(),
        Tab::Remotes => s.remotes.len(),
        Tab::Stash => s.stashes.len(),
        Tab::Tags => s.tags.len(),
        Tab::Worktrees => s.worktrees.len(),
        _ => return None,
    };
    (n > 0).then(|| n.to_string())
}

fn render_button(frame: &mut Frame, area: Rect, tab: Tab, app: &App, compact: bool) {
    let theme = &app.theme;
    let selected = tab == app.active_tab;
    let disabled = tab.needs_repo() && app.repo.is_none();
    let alarm = has_alarm(tab, app);
    let led_color = if alarm {
        theme.error
    } else if disabled {
        theme.fg_dim
    } else {
        theme.tab_accent(tab.index())
    };
    let glyph = if alarm || selected || (!disabled && is_live(tab, app)) { glyph::LED_ON } else { glyph::LED_OFF };
    let label_style = if selected {
        theme.style_for_bold()
    } else if disabled {
        theme.style_for_color(theme.fg_dim)
    } else {
        theme.tab_inactive_style()
    };

    let width = area.width.saturating_sub(if compact { 0 } else { 2 }) as usize;
    let title = tab.title(app.lang).to_uppercase();
    let badge = badge(tab, app).unwrap_or_default();
    let used = 3 + 2 + badge.chars().count() + 1;
    let title = format::truncate(&title, width.saturating_sub(used));
    let gap = width.saturating_sub(used + title.chars().count());
    let line = Line::from(vec![
        Span::styled(format!(" {glyph} "), theme.style_for_color(led_color)),
        Span::styled(format!("{} ", tab.hotkey()), theme.label_style()),
        Span::styled(title, label_style),
        Span::raw(" ".repeat(gap)),
        Span::styled(badge, theme.readout_style()),
    ]);

    if compact {
        let style = if selected { theme.raised_style() } else { theme.base_style() };
        frame.render_widget(Paragraph::new(line).style(style), area);
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if selected { BorderType::Thick } else { BorderType::Plain })
        .border_style(theme.border_style(selected))
        .style(if selected { theme.raised_style() } else { theme.panel_style() });
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_legend(frame: &mut Frame, area: Rect, app: &App) {
    if area.height < 3 {
        return;
    }
    let theme = &app.theme;
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" F2 ", theme.plate_style(theme.accent_primary)),
            Span::styled(format!(" {}", app.t(Key::SidebarConfig)), theme.label_style()),
        ]),
        Line::from(vec![
            Span::styled(" H  ", theme.plate_style(theme.accent_secondary)),
            Span::styled(format!(" {}", app.t(Key::SidebarHelp)), theme.label_style()),
        ]),
    ];
    if let Some(repo) = &app.repo {
        let path = shorten_home(&repo.ctx.root);
        lines.push(Line::from(Span::styled(format!(" {}", format::truncate_left(&path, area.width.saturating_sub(2) as usize)), theme.label_style())));
    }
    frame.render_widget(Paragraph::new(lines).style(theme.base_style()), area);
}
