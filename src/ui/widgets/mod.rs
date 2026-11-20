pub mod header;
pub mod help;
pub mod help_bar;
pub mod list;
pub mod modals;
pub mod panel;
pub mod sidebar;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::App;
use crate::theme::glyph;

/// Panel vacío con un mensaje centrado (sin repo, lista vacía...).
pub fn empty(frame: &mut Frame, area: Rect, app: &App, message: &str) {
    let theme = &app.theme;
    let block = panel::block(theme, "—", theme.border, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let y = inner.y + inner.height / 2;
    let line_area = Rect { y, height: 1.min(inner.height), ..inner };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("{} ", glyph::LED_OFF), theme.label_style()),
            Span::styled(message.to_string(), theme.label_style()),
        ]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        line_area,
    );
}
