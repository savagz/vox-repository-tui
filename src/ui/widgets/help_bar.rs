use ratatui::{
    Frame,
    layout::Rect,
    style::Color,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::App;
use crate::i18n::Key;
use crate::theme::Theme;
use crate::ui::tabs::Tab;

/// Barra inferior: capuchones de tecla contextuales según la sección y el
/// foco. Un aviso reciente la ocupa hasta la próxima tecla.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    if let Some((message, status)) = &app.toast {
        let line = Line::from(vec![
            Span::styled(app.t(Key::HbNotice), theme.plate_style(theme.color_for(*status))),
            Span::styled(format!(" {message}"), theme.style_for(*status)),
        ]);
        frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: true }).style(theme.base_style()), area);
        return;
    }
    frame.render_widget(Paragraph::new(Line::from(build_spans(app))).wrap(Wrap { trim: true }).style(theme.base_style()), area);
}

pub fn height_for(app: &App, width: u16) -> u16 {
    let usable = width.max(1) as usize;
    let total: usize = match &app.toast {
        Some((m, _)) => app.t(Key::HbNotice).chars().count() + m.chars().count() + 1,
        None => build_spans(app).iter().map(|s| s.content.chars().count()).sum(),
    };
    let lines = total.saturating_add(usable - 1) / usable;
    (lines.max(1) as u16).min(3)
}

type Entry = (&'static str, Key, Color);

fn entries(app: &App) -> Vec<Entry> {
    let t = &app.theme;
    let (acc, ok, warn, err, info, sec) = (t.accent_primary, t.ok, t.warning, t.error, t.info, t.accent_secondary);
    let filtering = match app.active_tab {
        Tab::Workspace => app.ws.filter_editing,
        _ => app.repo.as_ref().is_some_and(|r| r.filter_editing),
    };
    if filtering {
        return vec![("ENTER", Key::HbApplyFilter, ok), ("ESC", Key::HbClearFilter, err)];
    }
    let repo = app.repo.as_ref();
    let mut v: Vec<Entry> = vec![("TAB", Key::HbView, sec), ("↑↓", Key::HbNav, sec)];
    match app.active_tab {
        Tab::Workspace => v.extend([
            ("ENTER", Key::HbOpen, acc),
            ("F", Key::HbFetch, info),
            ("⇧F", Key::HbFetchAll, info),
            ("P", Key::HbPull, ok),
            ("⇧P", Key::HbPullAll, ok),
            ("S", Key::HbSort, sec),
            ("D", Key::HbOnlyDirty, sec),
            ("/", Key::HbFilter, sec),
            ("!", Key::HbShell, sec),
        ]),
        Tab::Status if repo.is_some_and(|r| r.conflict.is_some()) => v.extend([
            ("O", Key::HbOurs, ok),
            ("T", Key::HbTheirs, info),
            ("B", Key::HbBoth, acc),
            ("U", Key::HbUnresolve, warn),
            ("ENTER", Key::HbWrite, ok),
            ("E", Key::HbEdit, sec),
            ("ESC", Key::HbBack, sec),
        ]),
        Tab::Status if repo.is_some_and(|r| r.diff_focus) => v.extend([
            ("SPACE", Key::HbLines, ok),
            ("A", Key::HbHunk, ok),
            ("V", Key::HbRange, sec),
            ("[ ]", Key::HbJumpHunk, sec),
            ("D", Key::HbDiscard, err),
            ("X", Key::HbToggleView, sec),
            ("ESC", Key::HbBack, sec),
        ]),
        Tab::Status => v.extend([
            ("SPACE", Key::HbStage, ok),
            ("A", Key::HbStageAll, ok),
            ("ENTER", Key::HbDiff, sec),
            ("C", Key::HbCommit, acc),
            ("⇧C", Key::HbCommitEditor, acc),
            ("⇧A", Key::HbAmend, warn),
            ("D", Key::HbDiscard, err),
            ("⇧D", Key::HbResetMenu, err),
            ("S", Key::HbStash, info),
            ("I", Key::HbIgnore, sec),
            ("E", Key::HbEdit, sec),
        ]),
        Tab::Commits if repo.is_some_and(|r| r.commit_files.is_some()) => v.extend([("PGUP/DN", Key::HbScroll, sec), ("ESC", Key::HbBack, sec)]),
        Tab::Commits => v.extend([
            ("ENTER", Key::HbFiles, sec),
            ("SPACE", Key::HbCheckout, acc),
            ("R", Key::HbReword, warn),
            ("⇧S", Key::HbSquash, warn),
            ("⇧F", Key::HbFixup, warn),
            ("D", Key::HbDrop, err),
            ("⇧J/⇧K", Key::HbMove, warn),
            ("I", Key::HbRebaseI, warn),
            ("⇧A", Key::HbAmendInto, warn),
            ("C", Key::HbCherry, info),
            ("⇧V", Key::HbRevert, info),
            ("X", Key::HbReset, err),
            ("N", Key::HbBranch, ok),
            ("T", Key::HbTag, ok),
            ("A", Key::HbAllBranches, sec),
            ("Y", Key::HbCopy, sec),
        ]),
        Tab::Branches => v.extend([
            ("SPACE", Key::HbCheckout, acc),
            ("ENTER", Key::HbLog, sec),
            ("N", Key::HbBranch, ok),
            ("D", Key::HbDelete, err),
            ("⇧R", Key::HbRename, sec),
            ("⇧M", Key::HbMerge, warn),
            ("R", Key::HbRebase, warn),
            ("U", Key::HbUpstream, info),
            ("⇧F", Key::HbFastForward, info),
        ]),
        Tab::Remotes => v.extend([
            ("N", Key::HbAdd, ok),
            ("F", Key::HbFetch, info),
            ("E", Key::HbUrl, sec),
            ("⇧R", Key::HbRename, sec),
            ("D", Key::HbDelete, err),
        ]),
        Tab::Stash => v.extend([
            ("SPACE", Key::HbApply, ok),
            ("G", Key::HbPop, ok),
            ("N", Key::HbBranch, info),
            ("S", Key::HbStash, info),
            ("D", Key::HbDelete, err),
        ]),
        Tab::Tags => v.extend([
            ("N", Key::HbTag, ok),
            ("SPACE", Key::HbCheckout, acc),
            ("ENTER", Key::HbLog, sec),
            ("⇧P", Key::HbPushTag, info),
            ("D", Key::HbDelete, err),
            ("⇧D", Key::HbDeleteRemote, err),
        ]),
        Tab::Worktrees => v.extend([
            ("ENTER", Key::HbOpen, acc),
            ("N", Key::HbAdd, ok),
            ("D", Key::HbDelete, err),
            ("X", Key::HbPrune, warn),
        ]),
        Tab::Reflog => v.extend([("SPACE", Key::HbCheckout, acc), ("N", Key::HbBranch, ok), ("X", Key::HbReset, err)]),
        Tab::SystemCheck => v.push(("F5", Key::HbRecheck, sec)),
        Tab::Overview => {}
    }
    if repo.is_some() && app.active_tab.needs_repo() {
        if !matches!(app.active_tab, Tab::Tags) {
            v.push(("P", Key::HbPull, ok));
            v.push(("⇧P", Key::HbPush, ok));
        }
        if !matches!(app.active_tab, Tab::Remotes) {
            v.push(("F", Key::HbFetch, info));
        }
        if repo.is_some_and(|r| r.snap.op != crate::git::model::OpState::None) {
            v.push(("M", Key::HbOpMenu, warn));
        }
        v.push((":", Key::HbCmd, sec));
    }
    v.extend([("L", Key::HbCmdLog, sec), ("H", Key::HbHelp, sec), ("Q", Key::HbQuit, err)]);
    v
}

fn build_spans(app: &App) -> Vec<Span<'static>> {
    entries(app).into_iter().flat_map(|(k, label, color)| [key(&app.theme, k, color), self::label(&app.theme, app.t(label))]).collect()
}

fn key(theme: &Theme, text: &str, color: Color) -> Span<'static> {
    Span::styled(format!(" {text} "), theme.plate_style(color))
}

fn label(theme: &Theme, text: &str) -> Span<'static> {
    Span::styled(format!(" {text}  "), theme.label_style())
}
