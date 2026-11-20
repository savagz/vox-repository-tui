use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::app::Follow;
use crate::git::GitResult;
use crate::git::model::{ChangedFile, OpState, RepoCtx, RepoSnapshot, RepoSummary, Status};

#[derive(Debug)]
pub enum AppEvent {
    Tick,
    Key(KeyEvent),
    /// La terminal cambió de tamaño: sólo hay que redibujar.
    Redraw,
    /// SIGTERM/SIGINT/SIGHUP: salir restaurando la terminal.
    Shutdown,

    /// Terminó el recorrido de carpetas del workspace.
    WsFound { scan: u64, paths: Vec<PathBuf> },
    WsSummary { scan: u64, path: PathBuf, result: GitResult<RepoSummary> },
    /// Fetch/pull lanzado desde el workspace.
    WsOpDone { path: PathBuf, desc: String, result: GitResult<String> },

    RepoOpened { result: GitResult<RepoCtx> },
    Snapshot { generation: u64, snap: Box<RepoSnapshot> },
    StatusPoll { generation: u64, status: GitResult<Status>, op: OpState },
    Detail { generation: u64, req: u64, content: DetailContent },
    OpDone { generation: u64, desc: String, result: GitResult<String>, follow: Follow },
}

#[derive(Debug)]
pub enum DetailContent {
    Text(GitResult<String>),
    Files(GitResult<Vec<ChangedFile>>),
}
