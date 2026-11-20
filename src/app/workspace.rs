//! Workspace: todos los repos bajo las raíces configuradas, con su estado
//! resumido y acciones en lote (fetch/pull).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::Semaphore;

use super::App;
use super::modal::TextInput;
use crate::event::AppEvent;
use crate::git::model::RepoSummary;
use crate::git::{Git, GitResult, load};
use crate::i18n::{self, Key};
use crate::theme::Status;

/// Repos resumidos a la vez: cada resumen corre 5 comandos git.
const PARALLEL: usize = 6;

#[derive(Debug, Clone)]
pub struct WsRepo {
    pub path: PathBuf,
    pub name: String,
    pub summary: Option<RepoSummary>,
    pub error: Option<String>,
    pub busy: bool,
}

impl WsRepo {
    pub fn is_dirty(&self) -> bool {
        self.summary.as_ref().is_some_and(|s| !s.counts.is_clean() || s.head.ahead > 0 || s.head.behind > 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WsSort {
    #[default]
    Name,
    Dirty,
    Recent,
}

impl WsSort {
    fn next(self) -> Self {
        match self {
            WsSort::Name => WsSort::Dirty,
            WsSort::Dirty => WsSort::Recent,
            WsSort::Recent => WsSort::Name,
        }
    }

    pub fn key(self) -> Key {
        match self {
            WsSort::Name => Key::SortName,
            WsSort::Dirty => Key::SortDirty,
            WsSort::Recent => Key::SortRecent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WsOp {
    Fetch,
    Pull,
}

pub struct WsState {
    pub repos: Vec<WsRepo>,
    pub sel: usize,
    pub scan: u64,
    pub scanning: bool,
    pub pending: usize,
    pub sort: WsSort,
    pub only_dirty: bool,
    pub filter: TextInput,
    pub filter_editing: bool,
    pub last_refresh: Instant,
    sem: Arc<Semaphore>,
}

impl WsState {
    pub fn new() -> Self {
        Self {
            repos: Vec::new(),
            sel: 0,
            scan: 0,
            scanning: false,
            pending: 0,
            sort: WsSort::default(),
            only_dirty: false,
            filter: TextInput::default(),
            filter_editing: false,
            last_refresh: Instant::now(),
            sem: Arc::new(Semaphore::new(PARALLEL)),
        }
    }

    /// Índices visibles, filtrados y ordenados.
    pub fn visible(&self) -> Vec<usize> {
        let f = self.filter.value.trim().to_lowercase();
        let mut idx: Vec<usize> = (0..self.repos.len())
            .filter(|&i| {
                let r = &self.repos[i];
                (f.is_empty() || r.name.to_lowercase().contains(&f)) && (!self.only_dirty || r.is_dirty())
            })
            .collect();
        match self.sort {
            WsSort::Name => idx.sort_by(|&a, &b| self.repos[a].name.to_lowercase().cmp(&self.repos[b].name.to_lowercase())),
            WsSort::Dirty => idx.sort_by_key(|&i| {
                let r = &self.repos[i];
                let weight = r.summary.as_ref().map(|s| s.counts.total() + (s.head.ahead + s.head.behind) as usize).unwrap_or(0);
                (std::cmp::Reverse(weight), r.name.to_lowercase())
            }),
            WsSort::Recent => idx.sort_by_key(|&i| std::cmp::Reverse(self.repos[i].summary.as_ref().and_then(|s| s.last_time).unwrap_or(0))),
        }
        idx
    }

    pub fn selected(&self) -> Option<&WsRepo> {
        self.visible().get(self.sel).map(|&i| &self.repos[i])
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected().map(|r| r.path.clone())
    }

    pub fn clamp(&mut self) {
        let len = self.visible().len();
        self.sel = self.sel.min(len.saturating_sub(1));
    }

    pub fn move_sel(&mut self, delta: isize) {
        self.sel = super::clamp_move(self.sel, delta, self.visible().len());
    }

    pub fn dirty_count(&self) -> usize {
        self.repos.iter().filter(|r| r.is_dirty()).count()
    }
}

fn display_name(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(rest) = path.strip_prefix(root) {
            let s = rest.display().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

impl App {
    pub(crate) fn rescan_workspace(&mut self) {
        self.ws.scan += 1;
        self.ws.scanning = true;
        let scan = self.ws.scan;
        let roots = self.config.roots();
        let depth = self.config.workspace.max_depth;
        let ignore = self.config.workspace.ignore.clone();
        self.spawn(async move {
            let paths = tokio::task::spawn_blocking(move || load::scan(&roots, depth, &ignore)).await.unwrap_or_default();
            AppEvent::WsFound { scan, paths }
        });
    }

    pub(crate) fn ws_found(&mut self, scan: u64, paths: Vec<PathBuf>) {
        if scan != self.ws.scan {
            return;
        }
        self.ws.scanning = false;
        let roots = self.config.roots();
        let old = std::mem::take(&mut self.ws.repos);
        self.ws.repos = paths
            .iter()
            .map(|p| {
                let prev = old.iter().find(|r| r.path == *p);
                WsRepo {
                    path: p.clone(),
                    name: display_name(p, &roots),
                    summary: prev.and_then(|r| r.summary.clone()),
                    error: None,
                    busy: false,
                }
            })
            .collect();
        self.ws.clamp();
        self.summarize(paths);
    }

    fn summarize(&mut self, paths: Vec<PathBuf>) {
        self.ws.last_refresh = Instant::now();
        let scan = self.ws.scan;
        for path in paths {
            self.ws.pending += 1;
            let git = Git::new(&path, self.config.git.command_timeout_secs, self.config.git.network_timeout_secs);
            let sem = self.ws.sem.clone();
            self.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let result = load::summary(&git).await;
                AppEvent::WsSummary { scan, path, result }
            });
        }
    }

    pub(crate) fn ws_summary(&mut self, scan: u64, path: PathBuf, result: GitResult<RepoSummary>) {
        self.ws.pending = self.ws.pending.saturating_sub(1);
        if scan != self.ws.scan {
            return;
        }
        if let Some(r) = self.ws.repos.iter_mut().find(|r| r.path == path) {
            match result {
                Ok(s) => {
                    r.summary = Some(s);
                    r.error = None;
                }
                Err(e) => r.error = Some(e),
            }
        }
    }

    /// Re-resume un repo (tras operar sobre él desde el panel).
    pub(crate) fn ws_refresh_one(&mut self, path: &Path) {
        if self.ws.repos.iter().any(|r| r.path == path) {
            self.summarize(vec![path.to_path_buf()]);
        }
    }

    pub(crate) fn poll_workspace(&mut self) {
        let secs = self.config.poll.workspace_secs.max(5);
        if self.active_tab != crate::ui::tabs::Tab::Workspace || self.ws.scanning || self.ws.pending > 0 {
            return;
        }
        if App::elapsed(self.ws.last_refresh, secs) {
            let paths = self.ws.repos.iter().map(|r| r.path.clone()).collect();
            self.summarize(paths);
        }
    }

    fn ws_op(&mut self, all: bool, op: WsOp) {
        let paths: Vec<PathBuf> = if all {
            self.ws.visible().iter().map(|&i| self.ws.repos[i].path.clone()).collect()
        } else {
            self.ws.selected_path().into_iter().collect()
        };
        if paths.is_empty() {
            return;
        }
        let desc_key = match op {
            WsOp::Fetch => Key::DescFetch,
            WsOp::Pull => Key::DescPullFf,
        };
        self.notify(i18n::fill(self.t(Key::WsBatchTpl), &[self.t(desc_key), &paths.len().to_string()]), Status::Info);
        for path in paths {
            if let Some(r) = self.ws.repos.iter_mut().find(|r| r.path == path) {
                if r.busy {
                    continue;
                }
                r.busy = true;
            }
            let git = Git::new(&path, self.config.git.command_timeout_secs, self.config.git.network_timeout_secs);
            let sem = self.ws.sem.clone();
            let name = self.ws.repos.iter().find(|r| r.path == path).map(|r| r.name.clone()).unwrap_or_default();
            let desc = format!("{} · {name}", self.t(desc_key));
            self.spawn(async move {
                let _permit = sem.acquire_owned().await;
                let result = match op {
                    WsOp::Fetch => git.run_net(&["fetch", "--all", "--prune"]).await,
                    WsOp::Pull => git.run_net(&["pull", "--ff-only"]).await,
                };
                AppEvent::WsOpDone { path, desc, result }
            });
        }
    }

    pub(crate) fn ws_op_done(&mut self, path: PathBuf, desc: String, result: GitResult<String>) {
        if let Some(r) = self.ws.repos.iter_mut().find(|r| r.path == path) {
            r.busy = false;
        }
        match &result {
            Ok(out) => {
                self.push_log(&desc, true, out);
                self.notify(i18n::fill(self.t(Key::OpOkTpl), &[&desc]), Status::Ok);
            }
            Err(e) => {
                self.push_log(&desc, false, e);
                let first = e.lines().next().unwrap_or(e);
                self.notify(i18n::fill(self.t(Key::OpErrTpl), &[&desc, first]), Status::Error);
            }
        }
        self.summarize(vec![path.clone()]);
        if self.repo.as_ref().is_some_and(|r| r.ctx.root == path) {
            self.refresh_repo(true);
        }
    }

    pub(crate) fn ws_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter | KeyCode::Right => {
                if let Some(path) = self.ws.selected_path() {
                    self.open_repo(path);
                }
            }
            KeyCode::Char('f') => self.ws_op(false, WsOp::Fetch),
            KeyCode::Char('F') => self.ws_op(true, WsOp::Fetch),
            KeyCode::Char('p') => self.ws_op(false, WsOp::Pull),
            KeyCode::Char('P') => self.ws_op(true, WsOp::Pull),
            KeyCode::Char('s') => {
                self.ws.sort = self.ws.sort.next();
                self.ws.sel = 0;
            }
            KeyCode::Char('d') => {
                self.ws.only_dirty = !self.ws.only_dirty;
                self.ws.sel = 0;
            }
            KeyCode::Char('y') => {
                if let Some(path) = self.ws.selected_path() {
                    self.copy_to_clipboard(&path.display().to_string());
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombre_relativo_a_la_raiz() {
        let roots = vec![PathBuf::from("/a/b")];
        assert_eq!(display_name(Path::new("/a/b/x/y"), &roots), "x/y");
        assert_eq!(display_name(Path::new("/a/b"), &roots), "b");
        assert_eq!(display_name(Path::new("/z/w"), &roots), "w");
    }
}
