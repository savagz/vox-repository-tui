pub mod branches;
pub mod commits;
pub mod overview;
pub mod reflog;
pub mod remotes;
pub mod stash;
pub mod status;
pub mod syscheck;
pub mod tags;
pub mod workspace;
pub mod worktrees;

use crate::i18n::{self, Key, Lang};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Tab {
    #[default]
    Workspace,
    Overview,
    Status,
    Commits,
    Branches,
    Remotes,
    Stash,
    Tags,
    Worktrees,
    Reflog,
    SystemCheck,
}

impl Tab {
    pub const ALL: [Tab; 11] = [
        Tab::Workspace,
        Tab::Overview,
        Tab::Status,
        Tab::Commits,
        Tab::Branches,
        Tab::Remotes,
        Tab::Stash,
        Tab::Tags,
        Tab::Worktrees,
        Tab::Reflog,
        Tab::SystemCheck,
    ];

    pub fn title(self, lang: Lang) -> &'static str {
        let key = match self {
            Tab::Workspace => Key::TabWorkspace,
            Tab::Overview => Key::TabOverview,
            Tab::Status => Key::TabStatus,
            Tab::Commits => Key::TabCommits,
            Tab::Branches => Key::TabBranches,
            Tab::Remotes => Key::TabRemotes,
            Tab::Stash => Key::TabStash,
            Tab::Tags => Key::TabTags,
            Tab::Worktrees => Key::TabWorktrees,
            Tab::Reflog => Key::TabReflog,
            Tab::SystemCheck => Key::TabSystemCheck,
        };
        i18n::t(lang, key)
    }

    /// Tecla de salto directo que se muestra en la botonera.
    pub fn hotkey(self) -> &'static str {
        match self {
            Tab::Workspace => "W",
            Tab::Overview => "1",
            Tab::Status => "2",
            Tab::Commits => "3",
            Tab::Branches => "4",
            Tab::Remotes => "5",
            Tab::Stash => "6",
            Tab::Tags => "7",
            Tab::Worktrees => "8",
            Tab::Reflog => "9",
            Tab::SystemCheck => "0",
        }
    }

    pub fn from_digit(d: u32) -> Option<Tab> {
        match d {
            0 => Some(Tab::SystemCheck),
            1..=9 => Some(Self::ALL[d as usize]),
            _ => None,
        }
    }

    /// Las pestañas del medio necesitan un repo abierto.
    pub fn needs_repo(self) -> bool {
        !matches!(self, Tab::Workspace | Tab::SystemCheck)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).expect("Tab siempre está en Tab::ALL")
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}
