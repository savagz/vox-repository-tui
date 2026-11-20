use ratatui::{Frame, layout::Rect};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    crate::ui::setup::render_panel(frame, area, &app.system_check, &app.theme);
}
