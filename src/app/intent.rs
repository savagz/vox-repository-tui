//! Intenciones del usuario. `start` decide si hace falta un formulario, una
//! confirmación o un menú antes; `run_intent` arma la operación git.

use super::modal::{CommitForm, Confirm, Field, Form, Menu, MenuItem, Modal};
use super::planner::Planner;
use super::{App, Follow};
use crate::git::model::OpState;
use crate::git::ops::{self, Op, git, net};
use crate::git::rebase::{Item, QuickEdit};
use crate::i18n::{self, Key};
use crate::theme::Status;
use crate::ui::tabs::Tab;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StashMode {
    All,
    Staged,
    KeepIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

impl ResetMode {
    fn flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        }
    }
}

/// Qué se descarta de un archivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardKind {
    Untracked,
    /// Sólo los cambios sin indexar.
    Worktree,
    /// Indexado y sin indexar, volviendo a HEAD.
    Everything,
    /// Archivo agregado al índice que no existe en HEAD.
    NewFile,
}

#[derive(Debug, Clone)]
pub enum Intent {
    // STATUS
    Commit { amend: bool },
    StageAll,
    UnstageAll,
    DiscardFile { path: String, kind: DiscardKind },
    DiscardSelection { patch: String },
    ResetMenu,
    DiscardAllUnstaged,
    CleanUntracked,
    ResetHardHead,
    UndoLastCommit,
    StashMenu,
    Stash { mode: StashMode },
    Ignore { path: String },
    TakeSide { path: String, ours: bool },
    // COMMITS
    Reword { hash: String, is_head: bool },
    Quick(QuickEdit),
    DropCommit { hash: String, subject: String },
    CheckoutRef { reference: String },
    ResetMenuTo { target: String },
    ResetTo { target: String, mode: ResetMode },
    Revert { hash: String, merge: bool, subject: String },
    CherryPick { hash: String, subject: String },
    NewBranch { from: Option<String> },
    NewTag { at: Option<String> },
    AmendInto { hash: String, subject: String },
    InteractiveRebase,
    // BRANCHES
    Checkout { name: String },
    CheckoutRemote { remote_ref: String, local: String },
    DeleteBranch { name: String, force: bool },
    DeleteRemoteBranch { remote: String, branch: String },
    RenameBranch { old: String },
    Merge { name: String },
    RebaseOnto { name: String },
    SetUpstream { branch: String },
    UnsetUpstream { branch: String },
    FastForward { branch: String, upstream: String },
    ViewLog { reference: String },
    // REMOTES
    AddRemote,
    RemoveRemote { name: String },
    RenameRemote { old: String },
    SetRemoteUrl { name: String, current: String },
    FetchRemote { name: String },
    // STASH
    StashApply { sel: String },
    StashPop { sel: String },
    StashDrop { sel: String },
    StashBranch { sel: String },
    // TAGS
    DeleteTag { name: String },
    PushTag { name: String },
    DeleteRemoteTag { name: String },
    // WORKTREES
    AddWorktree,
    RemoveWorktree { path: String, force: bool },
    PruneWorktrees,
    OpenWorktree { path: String },
    // GLOBAL
    Pull,
    Push,
    ForcePush,
    FetchAll,
    OpMenu,
    OpContinue,
    OpAbort,
    OpSkip,
    GitCommand,
    ExecuteRebase { base: Option<String>, items: Vec<Item> },
    PlannerReword { planner: Box<Planner>, idx: usize },
}

impl App {
    fn tf(&self, key: Key, args: &[&str]) -> String {
        i18n::fill(self.t(key), args)
    }

    fn confirm(&mut self, message: String, intent: Intent) {
        self.modal = Some(Modal::Confirm(Confirm::new(self.t(Key::ConfirmTitle), message, intent)));
    }

    fn confirm_typed(&mut self, message: String, expected: &str, intent: Intent) {
        self.modal = Some(Modal::Confirm(Confirm::new(self.t(Key::DangerTitle), message, intent).typed(expected)));
    }

    fn form(&mut self, title: Key, fields: Vec<Field>, intent: Intent) {
        self.modal = Some(Modal::Form(Form::new(self.t(title), fields, intent)));
    }

    fn menu(&mut self, title: Key, items: Vec<(char, Key, Intent)>) {
        let items = items.into_iter().map(|(key, label, intent)| MenuItem { key, label: self.t(label).to_string(), intent }).collect();
        self.modal = Some(Modal::Menu(Menu::new(self.t(title), items)));
    }

    fn head(&self) -> Option<String> {
        self.repo.as_ref().and_then(|r| r.head_branch().map(str::to_string))
    }

    fn unborn(&self) -> bool {
        self.repo.as_ref().is_some_and(|r| r.snap.status.head.oid.is_none())
    }

    /// Remoto por defecto: el del upstream de HEAD, `origin`, o el primero.
    fn default_remote(&self) -> Option<String> {
        let repo = self.repo.as_ref()?;
        if let Some(up) = &repo.snap.status.head.upstream
            && let Some((remote, _)) = up.split_once('/')
        {
            return Some(remote.to_string());
        }
        repo.snap
            .remotes
            .iter()
            .find(|r| r.name == "origin")
            .or_else(|| repo.snap.remotes.first())
            .map(|r| r.name.clone())
    }

    /// Mensaje completo de un commit (para editarlo). Lectura local y corta.
    fn commit_message(&self, hash: &str) -> String {
        let Some(repo) = &self.repo else { return String::new() };
        std::process::Command::new("git")
            .args(["log", "-1", "--format=%B", hash])
            .current_dir(&repo.ctx.root)
            .stdin(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
            .unwrap_or_default()
    }

    /// Punto de entrada de toda acción: abre lo que haga falta o la ejecuta.
    pub(crate) fn start(&mut self, intent: Intent) {
        use Intent::*;
        match intent {
            Commit { amend } => {
                let msg = if amend { self.commit_message("HEAD") } else { String::new() };
                self.modal = Some(Modal::Commit(CommitForm::new(&msg, amend, intent)));
            }
            DiscardFile { ref path, .. } => {
                let m = self.tf(Key::ConfirmDiscardTpl, &[path]);
                self.confirm(m, intent);
            }
            DiscardSelection { .. } => self.confirm(self.t(Key::ConfirmDiscardSel).into(), intent),
            ResetMenu => self.menu(
                Key::MenuReset,
                vec![
                    ('d', Key::MiDiscardUnstaged, DiscardAllUnstaged),
                    ('c', Key::MiClean, CleanUntracked),
                    ('u', Key::MiUndoCommit, UndoLastCommit),
                    ('h', Key::MiHardReset, ResetHardHead),
                ],
            ),
            DiscardAllUnstaged => self.confirm(self.t(Key::ConfirmDiscardAll).into(), intent),
            CleanUntracked => self.confirm_typed(self.t(Key::ConfirmClean).into(), "clean", intent),
            ResetHardHead => self.confirm_typed(self.t(Key::ConfirmHardHead).into(), "reset", intent),
            UndoLastCommit => self.confirm(self.t(Key::ConfirmUndoCommit).into(), intent),
            StashMenu => self.menu(
                Key::MenuStash,
                vec![
                    ('a', Key::MiStashAll, Stash { mode: StashMode::All }),
                    ('s', Key::MiStashStaged, Stash { mode: StashMode::Staged }),
                    ('k', Key::MiStashKeep, Stash { mode: StashMode::KeepIndex }),
                ],
            ),
            Stash { .. } => self.form(Key::FormStash, vec![Field::optional(self.t(Key::FieldMessage), "")], intent),
            Reword { ref hash, .. } => {
                let msg = self.commit_message(hash);
                self.modal = Some(Modal::Commit(CommitForm::new(&msg, false, intent)));
            }
            DropCommit { ref subject, .. } => {
                let m = self.tf(Key::ConfirmDropTpl, &[subject]);
                self.confirm(m, intent);
            }
            CheckoutRef { ref reference } => {
                let m = self.tf(Key::ConfirmCheckoutTpl, &[reference]);
                self.confirm(m, intent);
            }
            ResetMenuTo { ref target } => {
                let t = target.clone();
                self.menu(
                    Key::MenuResetTo,
                    vec![
                        ('s', Key::MiSoft, ResetTo { target: t.clone(), mode: ResetMode::Soft }),
                        ('m', Key::MiMixed, ResetTo { target: t.clone(), mode: ResetMode::Mixed }),
                        ('h', Key::MiHard, ResetTo { target: t, mode: ResetMode::Hard }),
                    ],
                );
            }
            ResetTo { ref target, mode } => {
                let short: String = target.chars().take(10).collect();
                let m = self.tf(Key::ConfirmResetTpl, &[mode.flag(), &short]);
                if mode == ResetMode::Hard {
                    self.confirm_typed(m, "reset", intent);
                } else {
                    self.confirm(m, intent);
                }
            }
            Revert { ref subject, .. } => {
                let m = self.tf(Key::ConfirmRevertTpl, &[subject]);
                self.confirm(m, intent);
            }
            CherryPick { ref subject, .. } => {
                let m = self.tf(Key::ConfirmCherryTpl, &[subject]);
                self.confirm(m, intent);
            }
            AmendInto { ref subject, .. } => {
                let m = self.tf(Key::ConfirmAmendIntoTpl, &[subject]);
                self.confirm(m, intent);
            }
            NewBranch { .. } => self.form(Key::FormNewBranch, vec![Field::text(self.t(Key::FieldName), "")], intent),
            NewTag { .. } => self.form(
                Key::FormNewTag,
                vec![Field::text(self.t(Key::FieldName), ""), Field::optional(self.t(Key::FieldTagMessage), "")],
                intent,
            ),
            InteractiveRebase => self.open_planner(),
            DeleteBranch { ref name, force } => {
                let m = self.tf(if force { Key::ConfirmForceDeleteTpl } else { Key::ConfirmDeleteBranchTpl }, &[name]);
                if force {
                    let n = name.clone();
                    self.confirm_typed(m, &n, intent);
                } else {
                    self.confirm(m, intent);
                }
            }
            DeleteRemoteBranch { ref remote, ref branch } => {
                let m = self.tf(Key::ConfirmDeleteRemoteTpl, &[&format!("{remote}/{branch}")]);
                let b = branch.clone();
                self.confirm_typed(m, &b, intent);
            }
            RenameBranch { ref old } => {
                let f = vec![Field::text(self.t(Key::FieldNewName), old.clone())];
                self.form(Key::FormRenameBranch, f, intent);
            }
            Merge { ref name } => {
                let m = self.tf(Key::ConfirmMergeTpl, &[name]);
                self.confirm(m, intent);
            }
            RebaseOnto { ref name } => {
                let m = self.tf(Key::ConfirmRebaseTpl, &[name]);
                self.confirm(m, intent);
            }
            SetUpstream { ref branch } => {
                let remote = self.default_remote().unwrap_or_else(|| "origin".into());
                let f = vec![Field::text(self.t(Key::FieldUpstream), format!("{remote}/{branch}"))];
                self.form(Key::FormUpstream, f, intent);
            }
            AddRemote => self.form(
                Key::FormAddRemote,
                vec![Field::text(self.t(Key::FieldName), "origin"), Field::text(self.t(Key::FieldUrl), "")],
                intent,
            ),
            RemoveRemote { ref name } => {
                let m = self.tf(Key::ConfirmRemoveRemoteTpl, &[name]);
                self.confirm(m, intent);
            }
            RenameRemote { ref old } => {
                let f = vec![Field::text(self.t(Key::FieldNewName), old.clone())];
                self.form(Key::FormRenameRemote, f, intent);
            }
            SetRemoteUrl { ref current, .. } => {
                let f = vec![Field::text(self.t(Key::FieldUrl), current.clone())];
                self.form(Key::FormRemoteUrl, f, intent);
            }
            StashDrop { ref sel } => {
                let m = self.tf(Key::ConfirmStashDropTpl, &[sel]);
                self.confirm(m, intent);
            }
            StashBranch { .. } => self.form(Key::FormStashBranch, vec![Field::text(self.t(Key::FieldName), "")], intent),
            DeleteTag { ref name } => {
                let m = self.tf(Key::ConfirmDeleteTagTpl, &[name]);
                self.confirm(m, intent);
            }
            DeleteRemoteTag { ref name } => {
                let m = self.tf(Key::ConfirmDeleteRemoteTagTpl, &[name]);
                let n = name.clone();
                self.confirm_typed(m, &n, intent);
            }
            AddWorktree => {
                let base = self.repo.as_ref().map(|r| format!("../{}-wt", r.ctx.name)).unwrap_or_default();
                let modes = vec![self.t(Key::WtNewBranch).to_string(), self.t(Key::WtExisting).to_string()];
                self.form(
                    Key::FormAddWorktree,
                    vec![Field::text(self.t(Key::FieldPath), base), Field::text(self.t(Key::FieldBranch), ""), Field::choice(self.t(Key::FieldMode), modes)],
                    intent,
                );
            }
            RemoveWorktree { ref path, force } => {
                let m = self.tf(Key::ConfirmRemoveWorktreeTpl, &[path]);
                if force {
                    self.confirm_typed(m, "force", intent);
                } else {
                    self.confirm(m, intent);
                }
            }
            ForcePush => self.confirm(self.t(Key::ConfirmForcePush).into(), intent),
            OpMenu => {
                let op = self.repo.as_ref().map(|r| r.snap.op).unwrap_or_default();
                let mut items = vec![];
                if op != OpState::Bisect {
                    items.push(('c', Key::MiContinue, OpContinue));
                }
                items.push(('a', Key::MiAbort, OpAbort));
                if matches!(op, OpState::Rebase | OpState::CherryPick | OpState::Revert) {
                    items.push(('s', Key::MiSkip, OpSkip));
                }
                self.menu(Key::MenuOp, items);
            }
            OpAbort => self.confirm(self.t(Key::ConfirmAbort).into(), intent),
            OpSkip => self.confirm(self.t(Key::ConfirmSkip).into(), intent),
            GitCommand => {
                let mut f = Form::new(self.t(Key::FormGitCommand), vec![Field::text("git", "")], intent);
                f.subtitle = self.t(Key::FormGitCommandHint).into();
                self.modal = Some(Modal::Form(f));
            }
            other => self.run_intent(other, Vec::new()),
        }
    }

    /// Cancelar un formulario del planificador vuelve al planificador.
    pub(crate) fn cancel_intent(&mut self, intent: Intent) {
        if let Intent::PlannerReword { planner, .. } = intent {
            self.modal = Some(Modal::Planner(planner));
        }
    }

    fn open_planner(&mut self) {
        let Some(repo) = &self.repo else { return };
        if repo.log_opts.reference.is_some() || repo.log_opts.all {
            self.notify(self.t(Key::PlannerHeadOnly), Status::Warning);
            return;
        }
        let Some(sel) = repo.selected(Tab::Commits) else { return };
        let commits: Vec<_> = repo.snap.log[..=sel].iter().filter_map(|r| r.commit.clone()).collect();
        if commits.iter().any(|c| c.parents.len() > 1) {
            self.notify(self.t(Key::PlannerMerges), Status::Warning);
            return;
        }
        let base = commits.last().and_then(|c| c.parents.first().cloned());
        let items: Vec<Item> = commits.iter().rev().map(|c| Item::pick(&c.hash, &c.subject)).collect();
        self.modal = Some(Modal::Planner(Box::new(Planner::new(base, items))));
    }

    pub(crate) fn run_intent(&mut self, intent: Intent, values: Vec<String>) {
        use Intent::*;
        let v = |i: usize| values.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        let (desc, op, follow): (String, Op, Follow) = match intent {
            Commit { amend } => {
                let mut args = vec!["commit".to_string(), "-m".into(), values.first().cloned().unwrap_or_default()];
                if amend {
                    args.push("--amend".into());
                }
                (self.t(if amend { Key::DescAmend } else { Key::DescCommit }).into(), Op::Git(args), Follow::None)
            }
            StageAll => (self.t(Key::DescStageAll).into(), git(["add", "-A"]), Follow::None),
            UnstageAll => {
                let op = if self.unborn() { git(["rm", "--cached", "-r", "-q", "."]) } else { git(["reset", "-q"]) };
                (self.t(Key::DescUnstageAll).into(), op, Follow::None)
            }
            DiscardFile { path, kind } => {
                let op = match kind {
                    DiscardKind::Untracked => git(["clean", "-f", "--", &path]),
                    DiscardKind::Worktree => git(["checkout", "--", &path]),
                    DiscardKind::Everything => git(["checkout", "HEAD", "--", &path]),
                    DiscardKind::NewFile => Op::Seq(vec![git(["rm", "--cached", "-q", "--", &path]), git(["clean", "-f", "--", &path])]),
                };
                (self.tf(Key::DescDiscardTpl, &[&path]), op, Follow::None)
            }
            DiscardSelection { patch } => {
                (self.t(Key::DescDiscardSel).into(), Op::ApplyPatch { patch, cached: false, reverse: true }, Follow::None)
            }
            DiscardAllUnstaged => (self.t(Key::DescDiscardAll).into(), git(["checkout", "--", "."]), Follow::None),
            CleanUntracked => (self.t(Key::DescClean).into(), git(["clean", "-fd"]), Follow::None),
            ResetHardHead => (self.t(Key::DescResetHard).into(), git(["reset", "--hard", "HEAD"]), Follow::None),
            UndoLastCommit => (self.t(Key::DescUndoCommit).into(), git(["reset", "--soft", "HEAD~1"]), Follow::None),
            Stash { mode } => {
                let mut args: Vec<String> = vec!["stash".into(), "push".into()];
                args.push(
                    match mode {
                        StashMode::All => "--include-untracked",
                        StashMode::Staged => "--staged",
                        StashMode::KeepIndex => "--keep-index",
                    }
                    .into(),
                );
                let msg = v(0);
                if !msg.is_empty() {
                    args.push("-m".into());
                    args.push(msg);
                }
                (self.t(Key::DescStash).into(), Op::Git(args), Follow::None)
            }
            Ignore { path } => {
                let line = format!("/{path}");
                (self.tf(Key::DescIgnoreTpl, &[&path]), Op::AppendLine { path: ".gitignore".into(), line }, Follow::None)
            }
            TakeSide { path, ours } => {
                let side = if ours { "--ours" } else { "--theirs" };
                let op = Op::Seq(vec![git(["checkout", side, "--", &path]), git(["add", "--", &path])]);
                (self.tf(Key::DescResolvedTpl, &[&path]), op, Follow::None)
            }
            Reword { hash, is_head } => {
                let message = values.first().cloned().unwrap_or_default();
                let op = if is_head {
                    Op::Git(vec!["commit".into(), "--amend".into(), "--only".into(), "-m".into(), message])
                } else {
                    Op::QuickRebase(QuickEdit::Reword { hash, message })
                };
                (self.t(Key::DescReword).into(), op, Follow::None)
            }
            Quick(edit) => (self.t(Key::DescRebase).into(), Op::QuickRebase(edit), Follow::None),
            DropCommit { hash, .. } => (self.t(Key::DescDrop).into(), Op::QuickRebase(QuickEdit::Drop { hash }), Follow::None),
            CheckoutRef { reference } => (self.tf(Key::DescCheckoutTpl, &[&reference]), git(["checkout", &reference]), Follow::None),
            ResetTo { target, mode } => {
                let short: String = target.chars().take(10).collect();
                (self.tf(Key::DescResetTpl, &[mode.flag(), &short]), git(["reset", mode.flag(), &target]), Follow::None)
            }
            Revert { hash, merge, .. } => {
                let mut args = vec!["revert", "--no-edit"];
                if merge {
                    args.extend(["-m", "1"]);
                }
                args.push(&hash);
                (self.t(Key::DescRevert).into(), git(args), Follow::None)
            }
            CherryPick { hash, .. } => (self.t(Key::DescCherryPick).into(), git(["cherry-pick", &hash]), Follow::None),
            NewBranch { from } => {
                let name = v(0);
                let mut args = vec!["checkout".to_string(), "-b".into(), name.clone()];
                if let Some(f) = from {
                    args.push(f);
                }
                (self.tf(Key::DescNewBranchTpl, &[&name]), Op::Git(args), Follow::None)
            }
            NewTag { at } => {
                let (name, msg) = (v(0), v(1));
                let mut args = vec!["tag".to_string()];
                if !msg.is_empty() {
                    args.extend(["-a".into(), name.clone(), "-m".into(), msg]);
                } else {
                    args.push(name.clone());
                }
                if let Some(at) = at {
                    args.push(at);
                }
                (self.tf(Key::DescNewTagTpl, &[&name]), Op::Git(args), Follow::None)
            }
            AmendInto { hash, .. } => (self.t(Key::DescAmendInto).into(), Op::AmendInto { hash }, Follow::None),
            Checkout { name } => (self.tf(Key::DescCheckoutTpl, &[&name]), git(["checkout", &name]), Follow::None),
            CheckoutRemote { remote_ref, local } => {
                let exists = self.repo.as_ref().is_some_and(|r| r.snap.branches.iter().any(|b| !b.is_remote && b.name == local));
                let op = if exists { git(["checkout", &local]) } else { git(["checkout", "-b", &local, "--track", &remote_ref]) };
                (self.tf(Key::DescCheckoutTpl, &[&local]), op, Follow::None)
            }
            DeleteBranch { name, force } => {
                let flag = if force { "-D" } else { "-d" };
                (self.tf(Key::DescDeleteBranchTpl, &[&name]), git(["branch", flag, &name]), Follow::None)
            }
            DeleteRemoteBranch { remote, branch } => {
                (self.tf(Key::DescDeleteBranchTpl, &[&format!("{remote}/{branch}")]), net(["push", &remote, "--delete", &branch]), Follow::None)
            }
            RenameBranch { old } => {
                let new = v(0);
                (self.tf(Key::DescRenameTpl, &[&old, &new]), git(["branch", "-m", &old, &new]), Follow::None)
            }
            Merge { name } => (self.tf(Key::DescMergeTpl, &[&name]), git(["merge", "--no-edit", &name]), Follow::None),
            RebaseOnto { name } => (self.tf(Key::DescRebaseOntoTpl, &[&name]), git(["rebase", "--autostash", &name]), Follow::None),
            SetUpstream { branch } => {
                let up = v(0);
                (self.tf(Key::DescUpstreamTpl, &[&branch, &up]), git(["branch", "--set-upstream-to", &up, &branch]), Follow::None)
            }
            UnsetUpstream { branch } => (self.tf(Key::DescUpstreamTpl, &[&branch, "-"]), git(["branch", "--unset-upstream", &branch]), Follow::None),
            FastForward { branch, upstream } => {
                let op = if self.head().as_deref() == Some(branch.as_str()) {
                    net(["pull", "--ff-only"])
                } else {
                    let Some((remote, rb)) = upstream.split_once('/') else { return };
                    net(["fetch", remote, &format!("{rb}:{branch}")])
                };
                (self.tf(Key::DescFastForwardTpl, &[&branch]), op, Follow::None)
            }
            ViewLog { reference } => {
                if let Some(repo) = &mut self.repo {
                    repo.log_opts.reference = Some(reference);
                    repo.log_opts.all = false;
                    repo.commit_files = None;
                    repo.sel[Tab::Commits.index()] = 0;
                }
                self.refresh_repo(true);
                self.goto_tab(Tab::Commits);
                return;
            }
            AddRemote => {
                let (name, url) = (v(0), v(1));
                (self.tf(Key::DescRemoteTpl, &[&name]), git(["remote", "add", &name, &url]), Follow::None)
            }
            RemoveRemote { name } => (self.tf(Key::DescRemoteTpl, &[&name]), git(["remote", "remove", &name]), Follow::None),
            RenameRemote { old } => {
                let new = v(0);
                (self.tf(Key::DescRenameTpl, &[&old, &new]), git(["remote", "rename", &old, &new]), Follow::None)
            }
            SetRemoteUrl { name, .. } => {
                let url = v(0);
                (self.tf(Key::DescRemoteTpl, &[&name]), git(["remote", "set-url", &name, &url]), Follow::None)
            }
            FetchRemote { name } => (format!("{} {name}", self.t(Key::DescFetch)), net(["fetch", "--prune", &name]), Follow::None),
            StashApply { sel } => (self.tf(Key::DescStashApplyTpl, &[&sel]), git(["stash", "apply", &sel]), Follow::None),
            StashPop { sel } => (self.tf(Key::DescStashPopTpl, &[&sel]), git(["stash", "pop", &sel]), Follow::None),
            StashDrop { sel } => (self.tf(Key::DescStashDropTpl, &[&sel]), git(["stash", "drop", &sel]), Follow::None),
            StashBranch { sel } => {
                let name = v(0);
                (self.tf(Key::DescNewBranchTpl, &[&name]), git(["stash", "branch", &name, &sel]), Follow::None)
            }
            DeleteTag { name } => (self.tf(Key::DescDeleteTagTpl, &[&name]), git(["tag", "-d", &name]), Follow::None),
            PushTag { name } => {
                let Some(remote) = self.default_remote() else {
                    self.notify(self.t(Key::NoRemote), Status::Warning);
                    return;
                };
                (self.tf(Key::DescPushTagTpl, &[&name]), net(["push", &remote, &format!("refs/tags/{name}")]), Follow::None)
            }
            DeleteRemoteTag { name } => {
                let Some(remote) = self.default_remote() else {
                    self.notify(self.t(Key::NoRemote), Status::Warning);
                    return;
                };
                (self.tf(Key::DescDeleteTagTpl, &[&format!("{remote}:{name}")]), net(["push", &remote, "--delete", &format!("refs/tags/{name}")]), Follow::None)
            }
            AddWorktree => {
                let (path, branch, mode) = (v(0), v(1), v(2));
                let op = if mode == self.t(Key::WtNewBranch) {
                    git(["worktree", "add", "-b", &branch, &path])
                } else {
                    git(["worktree", "add", &path, &branch])
                };
                (self.tf(Key::DescWorktreeTpl, &[&path]), op, Follow::None)
            }
            RemoveWorktree { path, force } => {
                let mut args = vec!["worktree", "remove"];
                if force {
                    args.push("--force");
                }
                args.push(&path);
                (self.tf(Key::DescWorktreeTpl, &[&path]), git(args), Follow::None)
            }
            PruneWorktrees => (self.t(Key::DescPrune).into(), git(["worktree", "prune", "-v"]), Follow::None),
            OpenWorktree { path } => {
                self.open_repo(path.into());
                return;
            }
            Pull => (self.t(Key::DescPull).into(), net(["pull"]), Follow::None),
            Push => {
                let Some(repo) = &self.repo else { return };
                let Some(branch) = repo.head_branch().map(str::to_string) else {
                    self.notify(self.t(Key::DetachedNoPush), Status::Warning);
                    return;
                };
                let op = if repo.snap.status.head.upstream.is_some() {
                    net(["push"])
                } else {
                    let Some(remote) = self.default_remote() else {
                        self.notify(self.t(Key::NoRemote), Status::Warning);
                        return;
                    };
                    net(["push", "-u", &remote, &branch])
                };
                (self.t(Key::DescPush).into(), op, Follow::OfferForcePush)
            }
            ForcePush => (self.t(Key::DescForcePush).into(), net(["push", "--force-with-lease"]), Follow::None),
            FetchAll => (self.t(Key::DescFetch).into(), net(["fetch", "--all", "--prune"]), Follow::None),
            OpContinue | OpAbort | OpSkip => {
                let op_state = self.repo.as_ref().map(|r| r.snap.op).unwrap_or_default();
                let Some(cmd) = op_state.command() else { return };
                let (args, key): (Vec<&str>, Key) = match (&intent, op_state) {
                    (OpAbort, OpState::Bisect) => (vec!["bisect", "reset"], Key::DescAbort),
                    (OpContinue, _) => (vec![cmd, "--continue"], Key::DescContinue),
                    (OpAbort, _) => (vec![cmd, "--abort"], Key::DescAbort),
                    _ => (vec![cmd, "--skip"], Key::DescSkip),
                };
                (format!("{} {}", self.t(key), op_state.label()), git(args), Follow::None)
            }
            GitCommand => {
                let line = values.first().cloned().unwrap_or_default();
                let mut args = match crate::git::split_args(&line) {
                    Ok(a) => a,
                    Err(e) => {
                        self.notify(e, Status::Error);
                        return;
                    }
                };
                if args.first().map(String::as_str) == Some("git") {
                    args.remove(0);
                }
                if args.is_empty() {
                    return;
                }
                let title = format!("git {}", args.join(" "));
                (title.clone(), Op::Net(args), Follow::ShowOutput(title))
            }
            ExecuteRebase { base, items } => (self.t(Key::DescRebase).into(), Op::Rebase { base, items }, Follow::None),
            PlannerReword { mut planner, idx } => {
                if let Some(item) = planner.items.get_mut(idx) {
                    let msg = v(0);
                    item.message = (!msg.is_empty()).then_some(msg);
                }
                self.modal = Some(Modal::Planner(planner));
                return;
            }
            // Los que sólo abren modales nunca llegan acá ya resueltos.
            ResetMenu | StashMenu | OpMenu | ResetMenuTo { .. } | InteractiveRebase => return,
        };
        self.run_op(desc, op, follow);
    }
}

/// Stage/unstage de un archivo completo según su estado.
pub(crate) fn toggle_file_op(entry: &crate::git::model::StatusEntry, unborn: bool) -> Op {
    if entry.has_unstaged() || entry.is_conflicted() {
        ops::git(["add", "-A", "--", entry.path.as_str()])
    } else if unborn {
        ops::git(["rm", "--cached", "-q", "--", entry.path.as_str()])
    } else {
        let mut args = vec!["reset".to_string(), "-q".into(), "--".into(), entry.path.clone()];
        if let Some(orig) = &entry.orig_path {
            args.push(orig.clone());
        }
        Op::Git(args)
    }
}
