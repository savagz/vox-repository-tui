use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Cell, Row},
};

use crate::app::App;
use crate::git::model::{Commit, LogRow};
use crate::i18n::Key;
use crate::theme::Theme;
use crate::ui::tabs::Tab;
use crate::ui::widgets::list::{self, TableSpec, cell};
use crate::ui::{format, split_list_detail};

/// Grafo con el nodo resaltado.
fn graph_spans(theme: &Theme, graph: &str) -> Vec<Span<'static>> {
    graph
        .chars()
        .map(|c| match c {
            '*' => Span::styled("●", theme.style_for_color(theme.accent_primary)),
            ' ' => Span::raw(" "),
            other => Span::styled(other.to_string(), theme.style_for_color(theme.fg_muted)),
        })
        .collect()
}

/// Decoraciones `HEAD -> main, origin/main, tag: v1` como placas de color.
pub fn ref_spans(theme: &Theme, refs: &str) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    for r in refs.split(", ").filter(|r| !r.is_empty()) {
        let (text, color) = if let Some(b) = r.strip_prefix("HEAD -> ") {
            (format!("HEAD→{b}"), theme.accent_primary)
        } else if r == "HEAD" {
            ("HEAD".to_string(), theme.accent_primary)
        } else if let Some(t) = r.strip_prefix("tag: ") {
            (t.to_string(), theme.accent_teal)
        } else if r.contains('/') {
            (r.to_string(), theme.accent_senary)
        } else {
            (r.to_string(), theme.accent_quinary)
        };
        out.push(Span::styled(format!("[{text}]"), theme.style_for_color(color)));
        out.push(Span::raw(" "));
    }
    out
}

fn commit_row<'a>(app: &App, graph: &str, c: &Commit, now: i64) -> Row<'a> {
    let theme = &app.theme;
    let mut subject = graph_spans(theme, graph);
    subject.extend(ref_spans(theme, &c.refs));
    subject.push(Span::styled(c.subject.clone(), theme.value_style()));
    Row::new(vec![
        cell(c.short.clone(), theme.readout_style()),
        Cell::from(Line::from(subject)),
        cell(format::truncate(&c.author, 16), theme.style_for_color(theme.accent_tertiary)),
        cell(format::ago(app.lang, c.time, now), theme.label_style()),
    ])
}

fn graph_row<'a>(app: &App, row: &LogRow) -> Row<'a> {
    Row::new(vec![Cell::from(""), Cell::from(Line::from(graph_spans(&app.theme, &row.graph))), Cell::from(""), Cell::from("")])
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let (left, right) = split_list_detail(area, 55);
    let accent = theme.tab_accent(Tab::Commits.index());

    if let Some(cf) = &repo.commit_files {
        let rows: Vec<Row> = cf
            .files
            .iter()
            .map(|f| {
                let color = match f.status {
                    'A' => theme.ok,
                    'D' => theme.error,
                    'R' | 'C' => theme.accent_quinary,
                    _ => theme.warning,
                };
                let path = match &f.orig_path {
                    Some(o) => format!("{o} → {}", f.path),
                    None => f.path.clone(),
                };
                Row::new(vec![cell(f.status.to_string(), theme.style_for_color(color)), cell(path, theme.value_style())])
            })
            .collect();
        let title = format!("{} · {}", app.t(Key::Files), cf.short);
        let empty = if cf.loading { app.t(Key::Loading) } else { cf.error.as_deref().unwrap_or(app.t(Key::NoFiles)) };
        list::table(
            frame,
            left,
            theme,
            TableSpec {
                title: &title,
                accent,
                readout: Some(format!("{}", cf.files.len())),
                header: vec!["", app.t(Key::ColFile)],
                widths: vec![Constraint::Length(1), Constraint::Fill(1)],
                rows,
                selected: (!cf.files.is_empty()).then_some(cf.sel),
                focused: true,
                empty,
            },
        );
        list::detail(frame, right, app, app.t(Key::Diff), accent);
        return;
    }

    let now = format::now();
    let visible = repo.visible(Tab::Commits);
    let sel = repo.sel[Tab::Commits.index()];
    let filtered = !repo.filter.value.trim().is_empty();
    let (rows, selected): (Vec<Row>, Option<usize>) = if filtered {
        let rows = visible.iter().filter_map(|&i| repo.snap.log[i].commit.as_ref().map(|c| commit_row(app, "", c, now))).collect();
        (rows, (!visible.is_empty()).then_some(sel))
    } else {
        let rows = repo
            .snap
            .log
            .iter()
            .map(|r| match &r.commit {
                Some(c) => commit_row(app, &r.graph, c, now),
                None => graph_row(app, r),
            })
            .collect();
        (rows, visible.get(sel).copied())
    };

    let title = match (&repo.log_opts.reference, repo.log_opts.all) {
        (_, true) => format!("{} · {}", app.t(Key::TabCommits), app.t(Key::AllBranches)),
        (Some(r), _) => format!("{} · {}", app.t(Key::TabCommits), r.trim_start_matches("refs/tags/")),
        _ => app.t(Key::TabCommits).to_string(),
    };
    let mut readout = format!("{}", visible.len());
    if filtered || repo.filter_editing {
        readout = format!("/{}{}", repo.filter.value, if repo.filter_editing { "▏" } else { "" });
    } else if repo.log_opts.reference.is_some() || repo.log_opts.all {
        readout = format!("{} · ESC HEAD", visible.len());
    }
    list::table(
        frame,
        left,
        theme,
        TableSpec {
            title: &title,
            accent,
            readout: Some(readout),
            header: vec!["SHA", app.t(Key::ColSubject), app.t(Key::ColAuthor), app.t(Key::ColAge)],
            widths: vec![Constraint::Length(8), Constraint::Fill(1), Constraint::Length(16), Constraint::Length(4)],
            rows,
            selected,
            focused: true,
            empty: if repo.loaded { app.t(Key::NoCommits) } else { app.t(Key::Loading) },
        },
    );
    list::detail(frame, right, app, app.t(Key::Detail), accent);
}
