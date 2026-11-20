pub mod format;
pub mod setup;
pub mod tabs;
pub mod widgets;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::app::App;
use crate::app::modal::Modal;
use crate::git::model::OpState;
use crate::i18n::{self, Key};
use crate::theme::{glyph, hazard_stripe};
use tabs::Tab;

const SIDEBAR_WIDTH: u16 = 24;
const HEADER_HEIGHT: u16 = 3;

pub fn draw(frame: &mut Frame, app: &App) {
    // Fondo de chapa uniforme: sin esto los huecos entre paneles muestran el
    // color por defecto de la terminal.
    frame.render_widget(Block::default().style(app.theme.base_style()), frame.area());

    let op = app.repo.as_ref().map(|r| r.snap.op).unwrap_or_default();
    let banner_height = if op != OpState::None { 1 } else { 0 };
    let help_bar_height = widgets::help_bar::height_for(app, frame.area().width);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Length(banner_height),
            Constraint::Min(0),
            Constraint::Length(help_bar_height),
        ])
        .split(frame.area());

    widgets::header::render(frame, rows[0], app);
    if banner_height > 0 {
        draw_op_banner(frame, rows[1], app, op);
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(0)])
        .split(rows[2]);

    widgets::sidebar::render(frame, cols[0], app);
    draw_body(frame, cols[1], app);
    widgets::help_bar::render(frame, rows[3], app);

    if let Some(modal) = &app.modal {
        let (theme, lang) = (&app.theme, app.lang);
        match modal {
            Modal::Form(f) => widgets::modals::form(frame, frame.area(), f, theme, lang),
            Modal::Confirm(c) => widgets::modals::confirm(frame, frame.area(), c, theme, lang),
            Modal::Menu(m) => widgets::modals::menu(frame, frame.area(), m, theme, lang),
            Modal::Output(o) => widgets::modals::output(frame, frame.area(), o, theme, lang),
            Modal::Commit(c) => widgets::modals::commit(frame, frame.area(), c, theme, lang),
            Modal::Planner(p) => widgets::modals::planner(frame, frame.area(), p, theme, lang),
            Modal::Config(c) => widgets::modals::config(frame, frame.area(), c, theme),
        }
    }
    if app.help_open {
        widgets::help::render(frame, frame.area(), app);
    }
}

/// Franja de alarma: hay una operación de varios pasos a medio camino.
fn draw_op_banner(frame: &mut Frame, area: Rect, app: &App, op: OpState) {
    let theme = &app.theme;
    let text = i18n::fill(app.t(Key::BannerOpTpl), &[op.label()]);
    let line = Line::from(vec![
        Span::styled(hazard_stripe(4), theme.style_for_color(theme.warning)),
        Span::styled(format!(" {} {} ", glyph::BOLT, text), theme.plate_style(theme.warning)),
        Span::styled(format!(" {}", app.t(Key::BannerOpHint)), theme.style_for_color(theme.warning)),
    ]);
    frame.render_widget(Paragraph::new(line).style(theme.base_style()), area);
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    if app.active_tab.needs_repo() && app.repo.is_none() {
        widgets::empty(frame, area, app, app.t(Key::NoRepo));
        return;
    }
    match app.active_tab {
        Tab::Workspace => tabs::workspace::render(frame, area, app),
        Tab::Overview => tabs::overview::render(frame, area, app),
        Tab::Status => tabs::status::render(frame, area, app),
        Tab::Commits => tabs::commits::render(frame, area, app),
        Tab::Branches => tabs::branches::render(frame, area, app),
        Tab::Remotes => tabs::remotes::render(frame, area, app),
        Tab::Stash => tabs::stash::render(frame, area, app),
        Tab::Tags => tabs::tags::render(frame, area, app),
        Tab::Worktrees => tabs::worktrees::render(frame, area, app),
        Tab::Reflog => tabs::reflog::render(frame, area, app),
        Tab::SystemCheck => tabs::syscheck::render(frame, area, app),
    }
}

/// Divide el cuerpo en lista (izquierda) y detalle (derecha); en terminales
/// angostas, arriba y abajo.
pub fn split_list_detail(area: Rect, list_pct: u16) -> (Rect, Rect) {
    let dir = if area.width >= 100 { Direction::Horizontal } else { Direction::Vertical };
    let parts = Layout::default()
        .direction(dir)
        .constraints([Constraint::Percentage(list_pct), Constraint::Percentage(100 - list_pct)])
        .split(area);
    (parts[0], parts[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::setup::SystemCheckState;
    use ratatui::{Terminal, backend::TestBackend};
    use tokio::sync::mpsc;

    fn app() -> App {
        let (tx, _rx) = mpsc::channel(10);
        let config = Config::default();
        let check = SystemCheckState::new(config.ui.language, crate::deps::Probe::default());
        App::new(config, tx, check)
    }

    /// Todas las pestañas se dibujan en terminales chicas y grandes sin
    /// panics de layout (restas de ancho saturadas).
    #[test]
    fn dibuja_todas_las_pestanas_en_varios_tamanos() {
        let mut app = app();
        for (w, h) in [(40, 12), (80, 24), (160, 50)] {
            for tab in Tab::ALL {
                app.active_tab = tab;
                let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| draw(f, &app)).unwrap();
            }
            app.help_open = true;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal.draw(|f| draw(f, &app)).unwrap();
            app.help_open = false;
        }
    }
}
