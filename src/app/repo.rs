//! Estado del repo abierto: snapshot, selección por pestaña, filtro, panel de
//! detalle y polling.

use std::path::PathBuf;
use std::time::Instant;

use super::conflicts::ConflictView;
use super::modal::TextInput;
use super::staging::Staging;
use super::App;
use crate::event::{AppEvent, DetailContent};
use crate::git::load::{self, LogOpts};
use crate::git::model::*;
use crate::git::{Git, GitResult};
use crate::i18n::{self, Key};
use crate::theme::Status as Led;
use crate::ui::tabs::Tab;

/// Qué muestra el panel de detalle; si cambia, se pide de nuevo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailKey {
    StatusFile { path: String, staged: bool, untracked: bool },
    Commit(String),
    CommitFile { hash: String, path: String, merge: bool },
    Branch(String),
    Remote(String),
    Stash(String),
    Tag(String),
    Worktree(String),
    Reflog(String),
}

#[derive(Debug, Clone, Default)]
pub struct Detail {
    pub key: Option<DetailKey>,
    pub req: u64,
    pub text: String,
    pub error: bool,
    pub scroll: u16,
    pub loading: bool,
}

impl Detail {
    pub fn line_count(&self) -> usize {
        self.text.lines().count()
    }
}

#[derive(Debug, Clone)]
pub struct CommitFiles {
    pub hash: String,
    pub short: String,
    pub merge: bool,
    pub files: Vec<ChangedFile>,
    pub sel: usize,
    pub req: u64,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct RepoState {
    pub ctx: RepoCtx,
    pub git: Git,
    pub generation: u64,
    pub snap: RepoSnapshot,
    pub loaded: bool,
    pub sel: [usize; Tab::ALL.len()],
    pub filter: TextInput,
    pub filter_editing: bool,
    /// En STATUS el foco está en el diff (indexado por hunk/línea).
    pub diff_focus: bool,
    /// Con cambios indexados y sin indexar, cuál diff se ve.
    pub staged_view: bool,
    pub staging: Option<Staging>,
    pub detail: Detail,
    pub commit_files: Option<CommitFiles>,
    pub conflict: Option<ConflictView>,
    pub log_opts: LogOpts,
    next_req: u64,
    pub last_status: Instant,
    pub last_snapshot: Instant,
    pub last_fetch: Instant,
    pub status_inflight: bool,
    pub snapshot_inflight: bool,
}

impl RepoState {
    fn new(ctx: RepoCtx, git: Git, generation: u64, log_limit: usize) -> Self {
        let now = Instant::now();
        Self {
            ctx,
            git,
            generation,
            snap: RepoSnapshot::default(),
            loaded: false,
            sel: [0; Tab::ALL.len()],
            filter: TextInput::default(),
            filter_editing: false,
            diff_focus: false,
            staged_view: false,
            staging: None,
            detail: Detail::default(),
            commit_files: None,
            conflict: None,
            log_opts: LogOpts { limit: log_limit, reference: None, all: false },
            next_req: 0,
            last_status: now,
            last_snapshot: now,
            last_fetch: now,
            status_inflight: false,
            snapshot_inflight: false,
        }
    }

    pub fn filter_clear(&mut self) {
        self.filter = TextInput::default();
        self.filter_editing = false;
    }

    fn req(&mut self) -> u64 {
        self.next_req += 1;
        self.next_req
    }

    fn matches(&self, fields: &[&str]) -> bool {
        let f = self.filter.value.trim().to_lowercase();
        f.is_empty() || fields.iter().any(|s| s.to_lowercase().contains(&f))
    }

    /// Índices visibles (con filtro) de la lista de cada pestaña.
    pub fn visible(&self, tab: Tab) -> Vec<usize> {
        let s = &self.snap;
        let idx = |n: usize, f: &dyn Fn(usize) -> bool| (0..n).filter(|i| f(*i)).collect::<Vec<_>>();
        match tab {
            Tab::Status => idx(s.status.entries.len(), &|i| self.matches(&[&s.status.entries[i].path])),
            Tab::Commits => idx(s.log.len(), &|i| {
                s.log[i].commit.as_ref().is_some_and(|c| self.matches(&[&c.subject, &c.author, &c.short, &c.refs]))
            }),
            Tab::Branches => idx(s.branches.len(), &|i| self.matches(&[&s.branches[i].name])),
            Tab::Remotes => idx(s.remotes.len(), &|i| self.matches(&[&s.remotes[i].name, &s.remotes[i].fetch_url])),
            Tab::Stash => idx(s.stashes.len(), &|i| self.matches(&[&s.stashes[i].message])),
            Tab::Tags => idx(s.tags.len(), &|i| self.matches(&[&s.tags[i].name, &s.tags[i].subject])),
            Tab::Worktrees => idx(s.worktrees.len(), &|i| {
                self.matches(&[&s.worktrees[i].path, s.worktrees[i].branch.as_deref().unwrap_or("")])
            }),
            Tab::Reflog => idx(s.reflog.len(), &|i| self.matches(&[&s.reflog[i].subject])),
            Tab::Workspace | Tab::Overview | Tab::SystemCheck => Vec::new(),
        }
    }

    pub fn visible_len(&self, tab: Tab) -> usize {
        if tab == Tab::Commits
            && let Some(cf) = &self.commit_files
        {
            return cf.files.len();
        }
        self.visible(tab).len()
    }

    /// Índice real del elemento seleccionado en la pestaña.
    pub fn selected(&self, tab: Tab) -> Option<usize> {
        self.visible(tab).get(self.sel[tab.index()]).copied()
    }

    pub fn selected_entry(&self) -> Option<&StatusEntry> {
        self.selected(Tab::Status).map(|i| &self.snap.status.entries[i])
    }

    pub fn selected_commit(&self) -> Option<&Commit> {
        self.selected(Tab::Commits).and_then(|i| self.snap.log[i].commit.as_ref())
    }

    pub fn selected_branch(&self) -> Option<&Branch> {
        self.selected(Tab::Branches).map(|i| &self.snap.branches[i])
    }

    pub fn selected_remote(&self) -> Option<&Remote> {
        self.selected(Tab::Remotes).map(|i| &self.snap.remotes[i])
    }

    pub fn selected_stash(&self) -> Option<&Stash> {
        self.selected(Tab::Stash).map(|i| &self.snap.stashes[i])
    }

    pub fn selected_tag(&self) -> Option<&Tag> {
        self.selected(Tab::Tags).map(|i| &self.snap.tags[i])
    }

    pub fn selected_worktree(&self) -> Option<&Worktree> {
        self.selected(Tab::Worktrees).map(|i| &self.snap.worktrees[i])
    }

    pub fn selected_reflog(&self) -> Option<&ReflogEntry> {
        self.selected(Tab::Reflog).map(|i| &self.snap.reflog[i])
    }

    pub fn head_branch(&self) -> Option<&str> {
        self.snap.status.head.head.as_deref()
    }

    /// Diff (indexado o no) que corresponde mostrar para una entrada.
    pub fn diff_side(&self, e: &StatusEntry) -> bool {
        if e.has_staged() && e.has_unstaged() { self.staged_view } else { e.has_staged() }
    }

    /// Re-selecciona por identidad tras recargar una lista.
    fn reselect<T, K: PartialEq>(&mut self, tab: Tab, old: Option<K>, items: &[T], key: impl Fn(&T) -> K) {
        let Some(old) = old else { return };
        let visible = self.visible(tab);
        if let Some(pos) = visible.iter().position(|&i| key(&items[i]) == old) {
            self.sel[tab.index()] = pos;
        }
    }

    fn clamp_all(&mut self) {
        for tab in Tab::ALL {
            let len = self.visible(tab).len();
            let s = &mut self.sel[tab.index()];
            *s = (*s).min(len.saturating_sub(1));
        }
    }
}

impl App {
    pub(crate) fn open_repo(&mut self, path: PathBuf) {
        self.opening = true;
        let git = Git::new(&path, self.config.git.command_timeout_secs, self.config.git.network_timeout_secs);
        self.spawn(async move { AppEvent::RepoOpened { result: load::open(&git).await } });
    }

    pub(crate) fn repo_opened(&mut self, result: GitResult<RepoCtx>) {
        self.opening = false;
        let ctx = match result {
            Ok(ctx) => ctx,
            Err(e) => {
                self.notify(i18n::fill(self.t(Key::OpenFailedTpl), &[&e]), Led::Error);
                return;
            }
        };
        if self.repo.as_ref().is_some_and(|r| r.ctx == ctx) {
            self.goto_tab(Tab::Status);
            return;
        }
        let generation = self.next_generation();
        let git = Git::new(&ctx.root, self.config.git.command_timeout_secs, self.config.git.network_timeout_secs);
        self.busy = 0;
        self.repo = Some(RepoState::new(ctx, git, generation, self.config.git.log_limit));
        self.refresh_repo(true);
        self.goto_tab(Tab::Status);
    }

    /// Pide un snapshot completo. `force` ignora si ya hay uno en vuelo.
    pub(crate) fn refresh_repo(&mut self, force: bool) {
        let Some(repo) = &mut self.repo else { return };
        if repo.snapshot_inflight && !force {
            return;
        }
        repo.snapshot_inflight = true;
        repo.last_snapshot = Instant::now();
        repo.last_status = Instant::now();
        let (git, ctx, opts, generation) = (repo.git.clone(), repo.ctx.clone(), repo.log_opts.clone(), repo.generation);
        self.spawn(async move {
            let snap = load::snapshot(&git, &ctx, &opts).await;
            AppEvent::Snapshot { generation, snap: Box::new(snap) }
        });
    }

    pub(crate) fn poll_repo(&mut self) {
        let status_secs = self.config.poll.status_secs.max(1);
        let snapshot_secs = self.config.poll.snapshot_secs.max(2);
        let fetch_mins = self.config.poll.auto_fetch_mins;
        let busy = self.busy > 0;
        let Some(repo) = &mut self.repo else { return };
        if busy {
            return;
        }
        if fetch_mins > 0 && App::elapsed(repo.last_fetch, fetch_mins * 60) && !repo.snap.remotes.is_empty() {
            repo.last_fetch = Instant::now();
            self.run_op(self.t(Key::DescAutoFetch).to_string(), crate::git::ops::net(["fetch", "--all", "--prune"]), super::Follow::None);
            return;
        }
        if !repo.snapshot_inflight && App::elapsed(repo.last_snapshot, snapshot_secs) {
            self.refresh_repo(false);
            return;
        }
        if !repo.status_inflight && !repo.snapshot_inflight && App::elapsed(repo.last_status, status_secs) {
            repo.status_inflight = true;
            repo.last_status = Instant::now();
            let (git, git_dir, generation) = (repo.git.clone(), repo.ctx.git_dir.clone(), repo.generation);
            self.spawn(async move {
                let status = load::status(&git).await;
                AppEvent::StatusPoll { generation, status, op: OpState::detect(&git_dir) }
            });
        }
    }

    pub(crate) fn apply_snapshot(&mut self, generation: u64, snap: RepoSnapshot) {
        let Some(repo) = &mut self.repo else { return };
        if repo.generation != generation {
            return;
        }
        let old_path = repo.selected_entry().map(|e| e.path.clone());
        let old_hash = repo.selected_commit().map(|c| c.hash.clone());
        let old_branch = repo.selected_branch().map(|b| b.full_ref.clone());
        let first_load = !repo.loaded;

        repo.snapshot_inflight = false;
        repo.loaded = true;
        if let Some(e) = snap.errors.first() {
            tracing::warn!(error = %e, "snapshot parcial");
        }
        repo.snap = snap;
        repo.clamp_all();
        let entries = repo.snap.status.entries.clone();
        repo.reselect(Tab::Status, old_path, &entries, |e: &StatusEntry| e.path.clone());
        let log = repo.snap.log.clone();
        repo.reselect(Tab::Commits, old_hash, &log, |r: &LogRow| r.commit.as_ref().map(|c| c.hash.clone()).unwrap_or_default());
        let branches = repo.snap.branches.clone();
        repo.reselect(Tab::Branches, old_branch, &branches, |b: &Branch| b.full_ref.clone());

        if first_load && repo.snap.status.entries.is_empty() && self.active_tab == Tab::Status {
            self.active_tab = Tab::Overview;
        }
        self.refresh_detail(true);
    }

    pub(crate) fn apply_status(&mut self, generation: u64, status: GitResult<Status>, op: OpState) {
        let Some(repo) = &mut self.repo else { return };
        if repo.generation != generation {
            return;
        }
        repo.status_inflight = false;
        let Ok(status) = status else { return };
        let head_moved = status.head != repo.snap.status.head || op != repo.snap.op;
        if status == repo.snap.status && !head_moved {
            return;
        }
        let old_path = repo.selected_entry().map(|e| e.path.clone());
        repo.snap.status = status;
        repo.snap.op = op;
        repo.clamp_all();
        let entries = repo.snap.status.entries.clone();
        repo.reselect(Tab::Status, old_path, &entries, |e: &StatusEntry| e.path.clone());
        if head_moved {
            self.refresh_repo(false);
        } else {
            self.refresh_detail(true);
        }
    }

    /// Calcula qué debería mostrar el panel de detalle y lo pide si cambió.
    pub(crate) fn refresh_detail(&mut self, force: bool) {
        let tab = self.active_tab;
        let Some(repo) = &mut self.repo else { return };
        let key = match tab {
            Tab::Status => repo.selected_entry().map(|e| DetailKey::StatusFile {
                path: e.path.clone(),
                staged: repo.diff_side(e),
                untracked: e.is_untracked(),
            }),
            Tab::Commits => match &repo.commit_files {
                Some(cf) => cf.files.get(cf.sel).map(|f| DetailKey::CommitFile { hash: cf.hash.clone(), path: f.path.clone(), merge: cf.merge }),
                None => repo.selected_commit().map(|c| DetailKey::Commit(c.hash.clone())),
            },
            Tab::Branches => repo.selected_branch().map(|b| DetailKey::Branch(b.full_ref.clone())),
            Tab::Remotes => repo.selected_remote().map(|r| DetailKey::Remote(r.name.clone())),
            Tab::Stash => repo.selected_stash().map(|s| DetailKey::Stash(s.selector.clone())),
            Tab::Tags => repo.selected_tag().map(|t| DetailKey::Tag(t.name.clone())),
            Tab::Worktrees => repo.selected_worktree().map(|w| DetailKey::Worktree(w.path.clone())),
            Tab::Reflog => repo.selected_reflog().map(|r| DetailKey::Reflog(r.selector.clone())),
            _ => None,
        };
        let Some(key) = key else {
            if tab != Tab::Commits || repo.commit_files.is_none() {
                repo.detail = Detail { req: repo.detail.req, ..Detail::default() };
                if tab == Tab::Status {
                    repo.staging = None;
                    repo.diff_focus = false;
                }
            }
            return;
        };
        if !force && repo.detail.key.as_ref() == Some(&key) {
            return;
        }
        if repo.detail.key.as_ref() != Some(&key) {
            repo.detail.scroll = 0;
            repo.detail.text.clear();
        }
        let req = repo.req();
        repo.detail.req = req;
        repo.detail.key = Some(key.clone());
        repo.detail.loading = true;
        let (git, generation) = (repo.git.clone(), repo.generation);
        self.spawn(async move {
            let text = match &key {
                DetailKey::StatusFile { path, staged, untracked } => load::file_diff(&git, path, *staged, *untracked).await,
                DetailKey::Commit(h) => load::commit_show(&git, h).await,
                DetailKey::CommitFile { hash, path, merge } => load::commit_file_diff(&git, hash, path, *merge).await,
                DetailKey::Branch(r) => load::branch_log(&git, r).await,
                DetailKey::Remote(n) => load::remote_show(&git, n).await,
                DetailKey::Stash(s) => load::stash_show(&git, s).await,
                DetailKey::Tag(t) => load::ref_show(&git, &format!("refs/tags/{t}")).await,
                DetailKey::Worktree(p) => load::worktree_status(&git, p).await,
                DetailKey::Reflog(s) => load::ref_show(&git, s).await,
            };
            AppEvent::Detail { generation, req, content: DetailContent::Text(text) }
        });
    }

    /// Abre la lista de archivos de un commit (ENTER en COMMITS).
    pub(crate) fn open_commit_files(&mut self) {
        let Some(repo) = &mut self.repo else { return };
        let Some(c) = repo.selected_commit().cloned() else { return };
        let req = repo.req();
        let merge = c.parents.len() > 1;
        repo.commit_files = Some(CommitFiles {
            hash: c.hash.clone(),
            short: c.short.clone(),
            merge,
            files: Vec::new(),
            sel: 0,
            req,
            loading: true,
            error: None,
        });
        let (git, generation) = (repo.git.clone(), repo.generation);
        self.spawn(async move {
            let files = load::commit_files(&git, &c.hash, merge).await;
            AppEvent::Detail { generation, req, content: DetailContent::Files(files) }
        });
    }

    pub(crate) fn apply_detail(&mut self, generation: u64, req: u64, content: DetailContent) {
        let Some(repo) = &mut self.repo else { return };
        if repo.generation != generation {
            return;
        }
        match content {
            DetailContent::Files(files) => {
                let Some(cf) = repo.commit_files.as_mut().filter(|cf| cf.req == req) else { return };
                cf.loading = false;
                match files {
                    Ok(f) => cf.files = f,
                    Err(e) => cf.error = Some(e),
                }
                self.refresh_detail(false);
            }
            DetailContent::Text(text) => {
                if repo.detail.req != req {
                    return;
                }
                repo.detail.loading = false;
                let (text, error) = match text {
                    Ok(t) => (t, false),
                    Err(e) => (e, true),
                };
                repo.detail.error = error;
                if let Some(DetailKey::StatusFile { path, staged, untracked }) = &repo.detail.key {
                    let prev_cursor = repo.staging.as_ref().filter(|s| s.path == *path && s.staged == *staged).map(|s| s.cursor);
                    let mut staging = Staging::new(path.clone(), *staged, *untracked, &text);
                    if let Some(c) = prev_cursor {
                        staging.restore_cursor(c);
                    }
                    if staging.rows.is_empty() {
                        repo.diff_focus = false;
                    }
                    repo.staging = Some(staging);
                }
                let max = text.lines().count().saturating_sub(1) as u16;
                repo.detail.scroll = repo.detail.scroll.min(max);
                repo.detail.text = text;
            }
        }
    }
}
