use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Color,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::App;
use crate::config::shorten_home;
use crate::git::model::OpState;
use crate::i18n::{self, Key};
use crate::theme::{Theme, glyph, segmented_bar};
use crate::ui::format;
use crate::ui::widgets::panel;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(repo) = &app.repo else { return };
    let theme = &app.theme;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Length(6), Constraint::Min(4)])
        .split(area);
    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)])
        .split(rows[0]);

    let snap = &repo.snap;
    let head = &snap.status.head;
    let now = format::now();
    let label = |k: Key| Span::styled(format!("{:<15}", app.t(k)), theme.label_style());

    // HEAD
    let block = panel::block(theme, "HEAD", theme.ok, false);
    let inner = block.inner(top[0]);
    frame.render_widget(block, top[0]);
    let branch = head.head.clone().unwrap_or_else(|| format!("({})", app.t(Key::Detached)));
    let last = snap.log.iter().find_map(|r| r.commit.as_ref());
    let mut lines = vec![
        Line::from(vec![label(Key::ColBranch), Span::styled(format!("{} {branch}", glyph::BRANCH), theme.style_for_bold())]),
        Line::from(vec![label(Key::ColUpstream), Span::styled(head.upstream.clone().unwrap_or_else(|| "—".into()), theme.value_style())]),
        Line::from(vec![
            label(Key::ColSync),
            Span::styled(format!("{}{}  {}{}", glyph::UP, head.ahead, glyph::DOWN, head.behind), theme.readout_style()),
        ]),
        Line::from(vec![label(Key::ColPath), Span::styled(shorten_home(&repo.ctx.root), theme.value_style())]),
    ];
    if let Some(c) = last {
        lines.push(Line::from(vec![
            label(Key::ColLastCommit),
            Span::styled(format!("{} ", c.short), theme.readout_style()),
            Span::styled(format!("{} ", format::ago(app.lang, c.time, now)), theme.label_style()),
            Span::styled(c.subject.clone(), theme.value_style()),
        ]));
    }
    if snap.op != OpState::None {
        lines.push(Line::from(Span::styled(format!("{} {}", glyph::BOLT, i18n::fill(app.t(Key::BannerOpTpl), &[snap.op.label()])), theme.style_for_color(theme.warning))));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    // WORKTREE
    let c = snap.status.counts();
    let block = panel::block(theme, app.t(Key::Worktree), theme.accent_secondary, false);
    let inner = block.inner(top[1]);
    frame.render_widget(block, top[1]);
    let total = c.total().max(1) as f64;
    let bar_w = inner.width.saturating_sub(18).max(4) as usize;
    let meter = |name: &str, n: usize, color: Color| {
        let (full, empty) = segmented_bar(n as f64 * 100.0 / total, bar_w);
        Line::from(vec![
            Span::styled(format!("{name:<12}"), theme.label_style()),
            Span::styled(format!("{n:>4} "), theme.readout_style()),
            Span::styled(full, theme.style_for_color(color)),
            Span::styled(empty, theme.style_for_color(theme.fg_dim)),
        ])
    };
    let lines = vec![
        meter(app.t(Key::Staged), c.staged, theme.ok),
        meter(app.t(Key::Unstaged), c.unstaged, theme.warning),
        meter(app.t(Key::Untracked), c.untracked, theme.accent_tertiary),
        meter(app.t(Key::Conflicts), c.conflicted, theme.error),
        Line::from(Span::styled(
            if c.is_clean() { format!("{} {}", glyph::LED_ON, app.t(Key::Clean)) } else { String::new() },
            theme.style_for_color(theme.ok),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    // INVENTARIO
    let block = panel::block(theme, app.t(Key::Inventory), theme.accent_quinary, false);
    let inner = block.inner(top[2]);
    frame.render_widget(block, top[2]);
    let local = snap.branches.iter().filter(|b| !b.is_remote).count();
    let remote = snap.branches.len() - local;
    let count = |k: Key, n: usize| Line::from(vec![label(k), Span::styled(n.to_string(), theme.readout_style())]);
    let lines = vec![
        count(Key::TabBranches, local),
        count(Key::RemoteBranches, remote),
        count(Key::TabTags, snap.tags.len()),
        count(Key::TabStash, snap.stashes.len()),
        count(Key::TabRemotes, snap.remotes.len()),
        count(Key::TabWorktrees, snap.worktrees.len()),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    render_activity(frame, rows[1], app, theme);
    render_recent(frame, rows[2], app, theme);
}

fn render_activity(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(repo) = &app.repo else { return };
    let snap = &repo.snap;
    let total: u32 = snap.activity.iter().sum();
    let block = panel::block(theme, app.t(Key::Activity), theme.accent_primary, false)
        .title(panel::readout(theme, &i18n::fill(app.t(Key::ActivityTpl), &[&total.to_string()]), theme.accent_primary).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let spark = format::sparkline(&snap.activity);
    let authors: Vec<Span> = snap
        .authors
        .iter()
        .take(5)
        .flat_map(|(a, n)| [Span::styled(format!("{a} "), theme.value_style()), Span::styled(format!("{n}  "), theme.readout_style())])
        .collect();
    let lines = vec![
        Line::from(vec![Span::styled(format!("{:<10}", app.t(Key::Last30Days)), theme.label_style()), Span::styled(spark, theme.style_for_color(theme.accent_primary))]),
        Line::from(""),
        Line::from([vec![Span::styled(format!("{:<10}", app.t(Key::Authors)), theme.label_style())], authors].concat()),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_recent(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let Some(repo) = &app.repo else { return };
    let block = panel::block(theme, app.t(Key::RecentCommits), theme.accent_quaternary, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let now = format::now();
    let lines: Vec<Line> = repo
        .snap
        .log
        .iter()
        .filter_map(|r| r.commit.as_ref())
        .take(inner.height as usize)
        .map(|c| {
            Line::from(vec![
                Span::styled(format!("{} ", c.short), theme.readout_style()),
                Span::styled(format!("{:>4} ", format::ago(app.lang, c.time, now)), theme.label_style()),
                Span::styled(format!("{} ", format::pad(&c.author, 14)), theme.style_for_color(theme.accent_tertiary)),
                Span::styled(c.subject.clone(), theme.value_style()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}
