use std::path::PathBuf;

/// Identidad de un repositorio abierto: raíz del árbol de trabajo y directorio
/// git real (distinto en worktrees secundarios).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoCtx {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Ordinary,
    Renamed,
    Unmerged,
    Untracked,
}

/// Una fila de `git status --porcelain=v2`. `index`/`worktree` son las letras
/// XY de git, con '.' para "sin cambios".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub orig_path: Option<String>,
    pub index: char,
    pub worktree: char,
    pub kind: EntryKind,
}

impl StatusEntry {
    pub fn is_conflicted(&self) -> bool {
        self.kind == EntryKind::Unmerged
    }

    pub fn is_untracked(&self) -> bool {
        self.kind == EntryKind::Untracked
    }

    pub fn has_staged(&self) -> bool {
        !self.is_untracked() && !self.is_conflicted() && self.index != '.'
    }

    pub fn has_unstaged(&self) -> bool {
        self.is_untracked() || self.is_conflicted() || self.worktree != '.'
    }

    /// Código corto de dos letras para la columna de estado.
    pub fn code(&self) -> String {
        match self.kind {
            EntryKind::Untracked => "??".into(),
            EntryKind::Unmerged => "UU".into(),
            _ => format!("{}{}", self.index, self.worktree).replace('.', " "),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchHead {
    /// `None` en un repo sin commits.
    pub oid: Option<String>,
    /// `None` con HEAD separado.
    pub head: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub head: BranchHead,
    pub entries: Vec<StatusEntry>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

impl StatusCounts {
    pub fn is_clean(&self) -> bool {
        self.staged + self.unstaged + self.untracked + self.conflicted == 0
    }

    pub fn total(&self) -> usize {
        self.staged + self.unstaged + self.untracked + self.conflicted
    }
}

impl Status {
    pub fn counts(&self) -> StatusCounts {
        let mut c = StatusCounts::default();
        for e in &self.entries {
            if e.is_conflicted() {
                c.conflicted += 1;
            } else if e.is_untracked() {
                c.untracked += 1;
            } else {
                if e.has_staged() {
                    c.staged += 1;
                }
                if e.has_unstaged() {
                    c.unstaged += 1;
                }
            }
        }
        c
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub author: String,
    pub time: i64,
    pub refs: String,
    pub subject: String,
}

/// Fila del log con grafo: las filas de sólo-grafo (`|\`) no tienen commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRow {
    pub graph: String,
    pub commit: Option<Commit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub full_ref: String,
    pub is_remote: bool,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
    pub short: String,
    pub time: i64,
    pub subject: String,
}

impl Branch {
    /// Para una rama remota `origin/feat/x` devuelve (`origin`, `feat/x`).
    pub fn remote_parts(&self) -> Option<(&str, &str)> {
        if !self.is_remote {
            return None;
        }
        self.name.split_once('/')
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stash {
    pub selector: String,
    pub time: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub short: String,
    pub annotated: bool,
    pub time: i64,
    pub subject: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflogEntry {
    pub short: String,
    pub selector: String,
    pub subject: String,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub status: char,
    pub path: String,
    pub orig_path: Option<String>,
}

/// Operación de varios pasos a medio camino, detectada por los archivos de
/// control dentro del directorio git.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpState {
    #[default]
    None,
    Rebase,
    Merge,
    CherryPick,
    Revert,
    Bisect,
}

impl OpState {
    pub fn detect(git_dir: &std::path::Path) -> Self {
        if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
            OpState::Rebase
        } else if git_dir.join("MERGE_HEAD").is_file() {
            OpState::Merge
        } else if git_dir.join("CHERRY_PICK_HEAD").is_file() {
            OpState::CherryPick
        } else if git_dir.join("REVERT_HEAD").is_file() {
            OpState::Revert
        } else if git_dir.join("BISECT_LOG").is_file() {
            OpState::Bisect
        } else {
            OpState::None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OpState::None => "",
            OpState::Rebase => "REBASE",
            OpState::Merge => "MERGE",
            OpState::CherryPick => "CHERRY-PICK",
            OpState::Revert => "REVERT",
            OpState::Bisect => "BISECT",
        }
    }

    /// Subcomando de git que continúa/aborta la operación.
    pub fn command(self) -> Option<&'static str> {
        match self {
            OpState::None => None,
            OpState::Rebase => Some("rebase"),
            OpState::Merge => Some("merge"),
            OpState::CherryPick => Some("cherry-pick"),
            OpState::Revert => Some("revert"),
            OpState::Bisect => Some("bisect"),
        }
    }
}

/// Todo lo que muestra el panel de un repositorio, cargado de una vez.
#[derive(Debug, Clone, Default)]
pub struct RepoSnapshot {
    pub status: Status,
    pub log: Vec<LogRow>,
    pub branches: Vec<Branch>,
    pub remotes: Vec<Remote>,
    pub stashes: Vec<Stash>,
    pub tags: Vec<Tag>,
    pub worktrees: Vec<Worktree>,
    pub reflog: Vec<ReflogEntry>,
    pub op: OpState,
    /// Commits por día de los últimos 30 días (índice 0 = hace 29 días).
    pub activity: Vec<u32>,
    /// Autores de los últimos 30 días con su cantidad de commits.
    pub authors: Vec<(String, u32)>,
    /// Errores de comandos individuales: el resto del snapshot sigue siendo útil.
    pub errors: Vec<String>,
}

/// Resumen liviano para la tabla del workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoSummary {
    pub head: BranchHead,
    pub counts: StatusCounts,
    pub last_time: Option<i64>,
    pub last_subject: String,
    pub origin: Option<String>,
    pub stashes: usize,
    pub op: OpState,
}
