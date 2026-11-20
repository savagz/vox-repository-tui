use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::i18n::{self, Key, Lang};
use crate::theme::Theme;

/// Tiempo relativo corto: "3m", "2h", "5d", "4w", "8mo", "2y".
pub fn ago(lang: Lang, ts: i64, now: i64) -> String {
    if ts <= 0 {
        return "—".into();
    }
    let s = (now - ts).max(0);
    let (n, unit) = match s {
        0..60 => return i18n::t(lang, Key::AgoNow).into(),
        60..3600 => (s / 60, "m"),
        3600..86_400 => (s / 3600, "h"),
        86_400..604_800 => (s / 86_400, "d"),
        604_800..2_592_000 => (s / 604_800, "w"),
        2_592_000..31_536_000 => (s / 2_592_000, "mo"),
        _ => (s / 31_536_000, "y"),
    };
    format!("{n}{unit}")
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Recorta a `max` columnas con "…" (ancho por carácter, sin medir CJK).
pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Recorta por la izquierda: útil para rutas largas.
pub fn truncate_left(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max || max == 0 {
        return s.to_string();
    }
    let tail: String = s.chars().skip(count - max.saturating_sub(1)).collect();
    format!("…{tail}")
}

pub fn pad(s: &str, width: usize) -> String {
    let t = truncate(s, width);
    let n = t.chars().count();
    format!("{t}{}", " ".repeat(width.saturating_sub(n)))
}

/// Estilo de una línea de diff / `git show`.
pub fn diff_style(theme: &Theme, line: &str) -> Style {
    if line.starts_with("+++") || line.starts_with("---") {
        theme.style_for_bold()
    } else if line.starts_with('+') {
        theme.style_for_color(theme.ok)
    } else if line.starts_with('-') {
        theme.style_for_color(theme.error)
    } else if line.starts_with("@@") {
        theme.style_for_color(theme.accent_secondary)
    } else if line.starts_with("diff --git") || line.starts_with("commit ") {
        theme.plate_style(theme.accent_primary)
    } else if line.starts_with("index ")
        || line.starts_with("new file")
        || line.starts_with("deleted file")
        || line.starts_with("similarity")
        || line.starts_with("rename ")
        || line.starts_with('\\')
    {
        theme.label_style()
    } else if line.starts_with("Author:") || line.starts_with("Date:") || line.starts_with("Refs:") {
        theme.style_for_color(theme.accent_tertiary)
    } else if line.contains(" | ") && (line.ends_with('+') || line.ends_with('-') || line.contains("Bin")) {
        theme.style_for_color(theme.accent_senary)
    } else {
        theme.value_style()
    }
}

/// Línea de un diff ya coloreada; reemplaza tabs para no romper el layout.
pub fn diff_line(theme: &Theme, line: &str) -> Line<'static> {
    let text = line.replace('\t', "    ");
    Line::from(Span::styled(text, diff_style(theme, line)))
}

/// Mini-barra de actividad con bloques de ocho alturas.
pub fn sparkline(values: &[u32]) -> String {
    const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let max = values.iter().copied().max().unwrap_or(0);
    values
        .iter()
        .map(|&v| {
            if v == 0 {
                '·'
            } else if max == 0 {
                BARS[0]
            } else {
                BARS[((v as usize * 7) / max as usize).min(7)]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiempos_relativos() {
        assert_eq!(ago(Lang::En, 100, 130), "now");
        assert_eq!(ago(Lang::En, 0, 130), "—");
        assert_eq!(ago(Lang::En, 1000, 1000 + 7200), "2h");
        assert_eq!(ago(Lang::En, 1, 1 + 86_400 * 400), "1y");
    }

    #[test]
    fn recortes() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate_left("abcdef", 4), "…def");
        assert_eq!(pad("ab", 4), "ab  ");
        assert_eq!(sparkline(&[0, 1, 7]), "·▂█");
    }
}
