//! Teclas propias de cada pestaña. Devuelven `true` si consumieron la tecla;
//! si no, se procesa como tecla global.

use crossterm::event::{KeyCode, KeyEvent};

use super::intent::{DiscardKind, Intent, toggle_file_op};
use super::{App, Follow};
use crate::git::ops::Op;
use crate::git::patch::{self, Apply};
use crate::git::rebase::QuickEdit;
use crate::i18n::Key;
use crate::theme::Status;
use crate::ui::tabs::Tab;

impl App {
    pub(crate) fn tab_key(&mut self, key: KeyEvent) -> bool {
        if self.active_tab.needs_repo() && self.repo.is_none() {
            return false;
        }
        match self.active_tab {
            Tab::Workspace => self.ws_key(key),
            Tab::Status => self.status_key(key),
            Tab::Commits => self.commits_key(key),
            Tab::Branches => self.branches_key(key),
            Tab::Remotes => self.remotes_key(key),
            Tab::Stash => self.stash_key(key),
            Tab::Tags => self.tags_key(key),
            Tab::Worktrees => self.worktrees_key(key),
            Tab::Reflog => self.reflog_key(key),
            Tab::Overview | Tab::SystemCheck => false,
        }
    }

    fn status_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        if repo.conflict.is_some() {
            return self.conflict_key(key);
        }
        if repo.diff_focus {
            return self.diff_key(key);
        }
        let entry = repo.selected_entry().cloned();
        let counts = repo.snap.status.counts();
        let unborn = repo.snap.status.head.oid.is_none();
        match key.code {
            KeyCode::Char(' ') => {
                let Some(e) = entry else { return true };
                let key = if e.has_unstaged() || e.is_conflicted() { Key::DescStageTpl } else { Key::DescUnstageTpl };
                let desc = crate::i18n::fill(self.t(key), &[&e.path]);
                self.run_op(desc, toggle_file_op(&e, unborn), Follow::None);
            }
            KeyCode::Char('a') => {
                let intent = if counts.unstaged + counts.untracked + counts.conflicted > 0 { Intent::StageAll } else { Intent::UnstageAll };
                self.start(intent);
            }
            KeyCode::Char('c') => {
                if counts.staged == 0 {
                    self.notify(self.t(Key::NothingStaged), Status::Warning);
                } else {
                    self.start(Intent::Commit { amend: false });
                }
            }
            KeyCode::Char('C') => self.commit_with_editor(),
            KeyCode::Char('A') => {
                if unborn {
                    self.notify(self.t(Key::NoCommitsYet), Status::Warning);
                } else {
                    self.start(Intent::Commit { amend: true });
                }
            }
            KeyCode::Char('d') => {
                let Some(e) = entry else { return true };
                let kind = if e.is_untracked() {
                    DiscardKind::Untracked
                } else if e.has_unstaged() {
                    DiscardKind::Worktree
                } else if e.index == 'A' {
                    DiscardKind::NewFile
                } else {
                    DiscardKind::Everything
                };
                self.start(Intent::DiscardFile { path: e.path, kind });
            }
            KeyCode::Char('D') => self.start(Intent::ResetMenu),
            KeyCode::Char('s') => self.start(Intent::StashMenu),
            KeyCode::Char('i') => {
                if let Some(e) = entry {
                    self.start(Intent::Ignore { path: e.path });
                }
            }
            KeyCode::Char('e') => {
                if let Some(e) = entry {
                    self.open_editor(&e.path);
                }
            }
            KeyCode::Char('x') => self.toggle_staged_view(),
            KeyCode::Char('O') | KeyCode::Char('T') => {
                if let Some(e) = entry.filter(|e| e.is_conflicted()) {
                    self.start(Intent::TakeSide { path: e.path, ours: key.code == KeyCode::Char('O') });
                }
            }
            KeyCode::Enter | KeyCode::Right => {
                let Some(e) = entry else { return true };
                if e.is_conflicted() {
                    self.open_conflict();
                } else if let Some(repo) = &mut self.repo
                    && repo.staging.as_ref().is_some_and(|s| s.has_changes())
                {
                    repo.diff_focus = true;
                }
            }
            KeyCode::Char('y') => {
                if let Some(e) = entry {
                    self.copy_to_clipboard(&e.path);
                }
            }
            _ => return false,
        }
        true
    }

    fn toggle_staged_view(&mut self) {
        if let Some(repo) = &mut self.repo {
            repo.staged_view = !repo.staged_view;
        }
        self.refresh_detail(false);
    }

    fn diff_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &mut self.repo else { return false };
        let Some(staging) = &mut repo.staging else {
            repo.diff_focus = false;
            return true;
        };
        match key.code {
            KeyCode::Esc | KeyCode::Left => {
                if staging.anchor.is_some() {
                    staging.anchor = None;
                } else {
                    repo.diff_focus = false;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => staging.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => staging.move_cursor(1),
            KeyCode::PageUp => staging.move_cursor(-10),
            KeyCode::PageDown => staging.move_cursor(10),
            KeyCode::Char('[') => staging.jump_hunk(false),
            KeyCode::Char(']') => staging.jump_hunk(true),
            KeyCode::Char('v') => staging.anchor = if staging.anchor.is_some() { None } else { Some(staging.cursor) },
            KeyCode::Char(' ') => self.stage_selection(false),
            KeyCode::Char('a') => self.stage_selection(true),
            KeyCode::Char('d') => self.discard_selection(),
            KeyCode::Char('x') => self.toggle_staged_view(),
            KeyCode::Char('e') => {
                let path = staging.path.clone();
                self.open_editor(&path);
            }
            _ => return false,
        }
        true
    }

    fn stage_selection(&mut self, whole_hunk: bool) {
        let Some(repo) = &mut self.repo else { return };
        let Some(staging) = &mut repo.staging else { return };
        let Some((hunk, lines)) = (if whole_hunk {
            staging.hunk_at(staging.cursor).map(|h| (h, None))
        } else {
            staging.selection().map(|(h, l)| (h, Some(l)))
        }) else {
            return;
        };
        staging.anchor = None;
        let op = if staging.untracked {
            Op::StageNewFileLines { path: staging.path.clone(), hunk, lines }
        } else {
            let mode = if staging.staged { Apply::Reverse } else { Apply::Forward };
            let Some(p) = patch::build(&staging.diff, hunk, lines.as_deref(), mode) else { return };
            Op::ApplyPatch { patch: p, cached: true, reverse: staging.staged }
        };
        let key = if staging.staged { Key::DescUnstageTpl } else { Key::DescStageTpl };
        let path = staging.path.clone();
        let desc = crate::i18n::fill(self.t(key), &[&path]);
        self.run_op(desc, op, Follow::None);
    }

    fn discard_selection(&mut self) {
        let Some(repo) = &mut self.repo else { return };
        let Some(staging) = &mut repo.staging else { return };
        if staging.staged || staging.untracked {
            self.notify(self.t(Key::DiscardOnlyUnstaged), Status::Warning);
            return;
        }
        let Some((hunk, lines)) = staging.selection() else { return };
        let Some(p) = patch::build(&staging.diff, hunk, Some(&lines), Apply::Reverse) else { return };
        staging.anchor = None;
        self.start(Intent::DiscardSelection { patch: p });
    }

    /// ¿Se puede reescribir historia desde la vista actual de COMMITS?
    fn history_editable(&mut self) -> bool {
        let Some(repo) = &self.repo else { return false };
        if repo.log_opts.reference.is_some() || repo.log_opts.all {
            self.notify(self.t(Key::PlannerHeadOnly), Status::Warning);
            return false;
        }
        true
    }

    fn commits_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &mut self.repo else { return false };
        if let Some(cf) = &mut repo.commit_files {
            let n = cf.files.len();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => cf.sel = cf.sel.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => cf.sel = (cf.sel + 1).min(n.saturating_sub(1)),
                KeyCode::Home | KeyCode::Char('g') => cf.sel = 0,
                KeyCode::End | KeyCode::Char('G') => cf.sel = n.saturating_sub(1),
                KeyCode::Esc | KeyCode::Left => repo.commit_files = None,
                KeyCode::Char('y') => {
                    if let Some(f) = cf.files.get(cf.sel) {
                        let p = f.path.clone();
                        self.copy_to_clipboard(&p);
                    }
                    return true;
                }
                _ => return false,
            }
            self.refresh_detail(false);
            return true;
        }
        let Some(c) = repo.selected_commit().cloned() else { return false };
        let is_head = repo.log_opts.reference.is_none() && !repo.log_opts.all && repo.snap.status.head.oid.as_deref() == Some(c.hash.as_str());
        let staged = repo.snap.status.counts().staged;
        match key.code {
            KeyCode::Enter | KeyCode::Right => self.open_commit_files(),
            KeyCode::Char(' ') => self.start(Intent::CheckoutRef { reference: c.hash }),
            KeyCode::Char('n') => self.start(Intent::NewBranch { from: Some(c.hash) }),
            KeyCode::Char('t') => self.start(Intent::NewTag { at: Some(c.hash) }),
            KeyCode::Char('r') if self.history_editable() => self.start(Intent::Reword { hash: c.hash, is_head }),
            KeyCode::Char('S') if self.history_editable() => self.start(Intent::Quick(QuickEdit::Squash { hash: c.hash })),
            KeyCode::Char('F') if self.history_editable() => self.start(Intent::Quick(QuickEdit::Fixup { hash: c.hash })),
            KeyCode::Char('d') if self.history_editable() => self.start(Intent::DropCommit { hash: c.hash, subject: c.subject }),
            KeyCode::Char('K') if self.history_editable() => self.start(Intent::Quick(QuickEdit::MoveUp { hash: c.hash })),
            KeyCode::Char('J') if self.history_editable() => self.start(Intent::Quick(QuickEdit::MoveDown { hash: c.hash })),
            KeyCode::Char('i') => self.start(Intent::InteractiveRebase),
            KeyCode::Char('A') if self.history_editable() => {
                if staged == 0 {
                    self.notify(self.t(Key::NothingStaged), Status::Warning);
                } else {
                    self.start(Intent::AmendInto { hash: c.hash, subject: c.subject });
                }
            }
            KeyCode::Char('c') => self.start(Intent::CherryPick { hash: c.hash, subject: c.subject }),
            KeyCode::Char('V') => self.start(Intent::Revert { merge: c.parents.len() > 1, hash: c.hash, subject: c.subject }),
            KeyCode::Char('x') => self.start(Intent::ResetMenuTo { target: c.hash }),
            KeyCode::Char('y') => self.copy_to_clipboard(&c.hash),
            KeyCode::Char('a') => {
                if let Some(repo) = &mut self.repo {
                    repo.log_opts.all = !repo.log_opts.all;
                    repo.log_opts.reference = None;
                    repo.sel[Tab::Commits.index()] = 0;
                }
                self.refresh_repo(true);
            }
            KeyCode::Char('r' | 'S' | 'F' | 'd' | 'K' | 'J' | 'A') => {}
            _ => return false,
        }
        true
    }

    fn branches_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        let Some(b) = repo.selected_branch().cloned() else { return false };
        let local_name = b.remote_parts().map(|(_, n)| n.to_string());
        match key.code {
            KeyCode::Char(' ') => match local_name {
                Some(local) => self.start(Intent::CheckoutRemote { remote_ref: b.name, local }),
                None => self.start(Intent::Checkout { name: b.name }),
            },
            KeyCode::Enter | KeyCode::Right => self.start(Intent::ViewLog { reference: b.name }),
            KeyCode::Char('n') => self.start(Intent::NewBranch { from: Some(b.name) }),
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if b.is_head {
                    self.notify(self.t(Key::CannotDeleteHead), Status::Warning);
                } else if let Some((remote, branch)) = b.remote_parts() {
                    self.start(Intent::DeleteRemoteBranch { remote: remote.into(), branch: branch.into() });
                } else {
                    self.start(Intent::DeleteBranch { name: b.name, force: key.code == KeyCode::Char('D') });
                }
            }
            KeyCode::Char('R') if !b.is_remote => self.start(Intent::RenameBranch { old: b.name }),
            KeyCode::Char('M') => self.start(Intent::Merge { name: b.name }),
            KeyCode::Char('r') => self.start(Intent::RebaseOnto { name: b.name }),
            KeyCode::Char('u') if !b.is_remote => self.start(Intent::SetUpstream { branch: b.name }),
            KeyCode::Char('U') if !b.is_remote => self.start(Intent::UnsetUpstream { branch: b.name }),
            KeyCode::Char('F') => match b.upstream.clone() {
                Some(up) if !b.is_remote => self.start(Intent::FastForward { branch: b.name, upstream: up }),
                _ => self.notify(self.t(Key::NoUpstream), Status::Warning),
            },
            KeyCode::Char('y') => self.copy_to_clipboard(&b.name),
            KeyCode::Char('R' | 'u' | 'U') => {}
            _ => return false,
        }
        true
    }

    fn remotes_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        let r = repo.selected_remote().cloned();
        match (key.code, r) {
            (KeyCode::Char('n'), _) => self.start(Intent::AddRemote),
            (KeyCode::Char('d'), Some(r)) => self.start(Intent::RemoveRemote { name: r.name }),
            (KeyCode::Char('R'), Some(r)) => self.start(Intent::RenameRemote { old: r.name }),
            (KeyCode::Char('e'), Some(r)) => self.start(Intent::SetRemoteUrl { name: r.name, current: r.fetch_url }),
            (KeyCode::Char('f'), Some(r)) => self.start(Intent::FetchRemote { name: r.name }),
            (KeyCode::Char('y'), Some(r)) => self.copy_to_clipboard(&r.fetch_url),
            _ => return false,
        }
        true
    }

    fn stash_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        let s = repo.selected_stash().map(|s| s.selector.clone());
        match (key.code, s) {
            (KeyCode::Char(' '), Some(sel)) => self.start(Intent::StashApply { sel }),
            (KeyCode::Char('g'), Some(sel)) => self.start(Intent::StashPop { sel }),
            (KeyCode::Char('d'), Some(sel)) => self.start(Intent::StashDrop { sel }),
            (KeyCode::Char('n'), Some(sel)) => self.start(Intent::StashBranch { sel }),
            (KeyCode::Char('s'), _) => self.start(Intent::StashMenu),
            _ => return false,
        }
        true
    }

    fn tags_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        let t = repo.selected_tag().map(|t| t.name.clone());
        match (key.code, t) {
            (KeyCode::Char('n'), _) => self.start(Intent::NewTag { at: None }),
            (KeyCode::Char(' '), Some(name)) => self.start(Intent::CheckoutRef { reference: format!("refs/tags/{name}") }),
            (KeyCode::Char('d'), Some(name)) => self.start(Intent::DeleteTag { name }),
            (KeyCode::Char('P'), Some(name)) => self.start(Intent::PushTag { name }),
            (KeyCode::Char('D'), Some(name)) => self.start(Intent::DeleteRemoteTag { name }),
            (KeyCode::Enter, Some(name)) => self.start(Intent::ViewLog { reference: format!("refs/tags/{name}") }),
            (KeyCode::Char('y'), Some(name)) => self.copy_to_clipboard(&name),
            _ => return false,
        }
        true
    }

    fn worktrees_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        let w = repo.selected_worktree().cloned();
        let current = repo.ctx.root.display().to_string();
        match (key.code, w) {
            (KeyCode::Char('n'), _) => self.start(Intent::AddWorktree),
            (KeyCode::Char('x'), _) => self.start(Intent::PruneWorktrees),
            (KeyCode::Enter | KeyCode::Right, Some(w)) => self.start(Intent::OpenWorktree { path: w.path }),
            (KeyCode::Char('d') | KeyCode::Char('D'), Some(w)) => {
                if w.path == current {
                    self.notify(self.t(Key::CannotRemoveCurrentWt), Status::Warning);
                } else {
                    self.start(Intent::RemoveWorktree { path: w.path, force: key.code == KeyCode::Char('D') });
                }
            }
            (KeyCode::Char('y'), Some(w)) => self.copy_to_clipboard(&w.path),
            _ => return false,
        }
        true
    }

    fn reflog_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &self.repo else { return false };
        let r = repo.selected_reflog().map(|r| r.short.clone());
        match (key.code, r) {
            (KeyCode::Char(' '), Some(h)) => self.start(Intent::CheckoutRef { reference: h }),
            (KeyCode::Char('x'), Some(h)) => self.start(Intent::ResetMenuTo { target: h }),
            (KeyCode::Char('n'), Some(h)) => self.start(Intent::NewBranch { from: Some(h) }),
            (KeyCode::Char('y'), Some(h)) => self.copy_to_clipboard(&h),
            _ => return false,
        }
        true
    }
}
