use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, TableState},
};

use super::panel;
use crate::app::App;
use crate::i18n::Key;
use crate::theme::{Theme, glyph};
use crate::ui::format;

pub struct TableSpec<'a> {
    pub title: &'a str,
    pub accent: Color,
    pub readout: Option<String>,
    pub header: Vec<&'a str>,
    pub widths: Vec<Constraint>,
    pub rows: Vec<Row<'a>>,
    pub selected: Option<usize>,
    pub focused: bool,
    pub empty: &'a str,
}

/// Tabla estándar del tablero: placa de título, lectura a la derecha,
/// cabecera en relieve y fila seleccionada con marcador ▌.
pub fn table(frame: &mut Frame, area: Rect, theme: &Theme, spec: TableSpec<'_>) {
    let mut block = panel::block(theme, spec.title, spec.accent, spec.focused);
    if let Some(r) = &spec.readout {
        block = block.title(panel::readout(theme, r, spec.accent).right_aligned());
    }
    if spec.rows.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", glyph::LED_OFF), theme.label_style()),
                Span::styled(spec.empty.to_string(), theme.label_style()),
            ])),
            inner,
        );
        return;
    }
    let header = Row::new(spec.header.into_iter().map(|h| Cell::from(h.to_uppercase()))).style(theme.table_header_style());
    let table = Table::new(spec.rows, spec.widths)
        .header(header)
        .block(block)
        .row_highlight_style(theme.selected_row_style())
        .highlight_symbol(Line::from(Span::styled("▌", theme.style_for_color(spec.accent))))
        .highlight_spacing(ratatui::widgets::HighlightSpacing::Always);
    let mut state = TableState::default().with_selected(spec.selected);
    frame.render_stateful_widget(table, area, &mut state);
}

/// Panel de detalle del repo (diff, commit, log de rama...).
pub fn detail(frame: &mut Frame, area: Rect, app: &App, title: &str, accent: Color) {
    let theme = &app.theme;
    let Some(repo) = &app.repo else { return };
    let d = &repo.detail;
    let readout = if d.loading {
        format!("{} {}", glyph::SPINNER[(app.tick % 4) as usize], app.t(Key::Loading))
    } else {
        format!("{}/{}", if d.line_count() == 0 { 0 } else { d.scroll as usize + 1 }, d.line_count())
    };
    let block = panel::block(theme, title, accent, false).title(panel::readout(theme, &readout, theme.fg_muted).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if d.text.is_empty() {
        let msg = if d.loading { "" } else { app.t(Key::NoDetail) };
        frame.render_widget(Paragraph::new(Span::styled(msg, theme.label_style())), inner);
        return;
    }
    let lines: Vec<Line> = d
        .text
        .lines()
        .skip(d.scroll as usize)
        .take(inner.height as usize)
        .map(|l| {
            if d.error {
                Line::from(Span::styled(l.to_string(), theme.style_for_color(theme.error)))
            } else {
                format::diff_line(theme, l)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Celda con texto recortado.
pub fn cell<'a>(text: impl Into<String>, style: ratatui::style::Style) -> Cell<'a> {
    Cell::from(Span::styled(text.into(), style))
}
