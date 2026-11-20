use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

use super::panel;
use crate::app::App;
use crate::i18n::{self, Key, Lang};

enum Entry {
    Section(Key),
    Row(&'static str, Key),
    Blank,
}

fn entries() -> Vec<Entry> {
    use Entry::*;
    use Key::*;
    vec![
        Section(HelpGlobal),
        Row("TAB / ⇧TAB", HbView),
        Row("W  1-9  0", HelpJump),
        Row("↑↓ j k  g G", HbNav),
        Row("PGUP/PGDN ^U ^D", HbScroll),
        Row("/", HbFilter),
        Row("p  ⇧P  f", HelpSync),
        Row("m", HbOpMenu),
        Row(":", HbCmd),
        Row("!", HbShell),
        Row("L", HbCmdLog),
        Row("F5  ^R", HelpRefresh),
        Row("F2", SidebarConfig),
        Row("q  ^C", HbQuit),
        Blank,
        Section(TabWorkspace),
        Row("ENTER", HbOpen),
        Row("f  ⇧F", HelpWsFetch),
        Row("p  ⇧P", HelpWsPull),
        Row("s", HbSort),
        Row("d", HbOnlyDirty),
        Row("y", HbCopy),
        Blank,
        Section(TabStatus),
        Row("SPACE", HbStage),
        Row("a", HbStageAll),
        Row("ENTER", HelpEnterDiff),
        Row("c  ⇧C", HelpCommit),
        Row("⇧A", HbAmend),
        Row("d", HbDiscard),
        Row("⇧D", HbResetMenu),
        Row("s", HbStash),
        Row("i", HbIgnore),
        Row("e", HbEdit),
        Row("x", HbToggleView),
        Row("⇧O  ⇧T", HelpTakeSide),
        Section(HelpDiffFocus),
        Row("SPACE", HbLines),
        Row("a", HbHunk),
        Row("v", HbRange),
        Row("[  ]", HbJumpHunk),
        Row("d", HelpDiscardLines),
        Section(HelpConflicts),
        Row("o  t  b  u", HelpConflictKeys),
        Row("ENTER / w", HbWrite),
        Blank,
        Section(TabCommits),
        Row("ENTER", HbFiles),
        Row("SPACE", HbCheckout),
        Row("r", HbReword),
        Row("⇧S  ⇧F", HelpSquashFixup),
        Row("d", HbDrop),
        Row("⇧J  ⇧K", HbMove),
        Row("i", HelpPlanner),
        Row("⇧A", HbAmendInto),
        Row("c", HbCherry),
        Row("⇧V", HbRevert),
        Row("x", HbReset),
        Row("n  t", HelpBranchTag),
        Row("a", HbAllBranches),
        Row("y", HbCopy),
        Section(HelpPlannerKeys),
        Row("p r e s f d", HelpPlannerActions),
        Row("⇧J  ⇧K", HbMove),
        Row("ENTER", HelpPlannerRun),
        Blank,
        Section(TabBranches),
        Row("SPACE", HbCheckout),
        Row("ENTER", HbLog),
        Row("n", HbBranch),
        Row("d  ⇧D", HelpDeleteBranch),
        Row("⇧R", HbRename),
        Row("⇧M", HbMerge),
        Row("r", HbRebase),
        Row("u  ⇧U", HbUpstream),
        Row("⇧F", HbFastForward),
        Blank,
        Section(TabRemotes),
        Row("n", HbAdd),
        Row("f", HbFetch),
        Row("e", HbUrl),
        Row("⇧R", HbRename),
        Row("d", HbDelete),
        Blank,
        Section(TabStash),
        Row("SPACE", HbApply),
        Row("g", HbPop),
        Row("n", HbBranch),
        Row("s", HbStash),
        Row("d", HbDelete),
        Blank,
        Section(TabTags),
        Row("n", HbTag),
        Row("SPACE", HbCheckout),
        Row("ENTER", HbLog),
        Row("⇧P", HbPushTag),
        Row("d  ⇧D", HelpDeleteTag),
        Blank,
        Section(TabWorktrees),
        Row("ENTER", HbOpen),
        Row("n", HbAdd),
        Row("d  ⇧D", HelpDeleteWorktree),
        Row("x", HbPrune),
        Blank,
        Section(TabReflog),
        Row("SPACE", HbCheckout),
        Row("n", HbBranch),
        Row("x", HbReset),
        Blank,
        Section(HelpClose),
    ]
}

pub fn line_count(_lang: Lang) -> u16 {
    entries().len() as u16
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let lang = app.lang;
    let modal = panel::centered(area, 76, 86);
    frame.render_widget(Clear, modal);
    let block = panel::block(theme, i18n::t(lang, Key::HelpTitle), theme.accent_primary, true);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let lines: Vec<Line> = entries()
        .into_iter()
        .map(|e| match e {
            Entry::Section(k) => Line::from(Span::styled(i18n::t(lang, k).to_uppercase(), theme.plate_style(theme.accent_primary))),
            Entry::Row(keys, k) => Line::from(vec![
                Span::styled(format!("  {keys:<18}"), theme.style_for_color(theme.accent_secondary)),
                Span::styled(i18n::t(lang, k), theme.label_style()),
            ]),
            Entry::Blank => Line::from(""),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).scroll((app.help_scroll, 0)), inner);
}
