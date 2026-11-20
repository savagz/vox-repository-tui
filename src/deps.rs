use std::path::{Path, PathBuf};

use crate::git;
use crate::i18n::{self, Key, Lang};
use crate::theme::Status;

/// Versión mínima recomendada: `stash push --staged` (2.35).
const RECOMMENDED: (u32, u32) = (2, 35);

#[derive(Debug, Clone)]
pub struct DepCheck {
    pub label: &'static str,
    pub detail: String,
    pub status: Status,
}

#[derive(Debug, Clone)]
pub struct DepReport {
    pub checks: Vec<DepCheck>,
}

impl DepReport {
    /// Sólo la falta de git bloquea: sin el binario no hay nada que mostrar.
    pub fn all_blocking_ok(&self) -> bool {
        self.checks.iter().all(|c| c.status != Status::Error)
    }

    pub fn get(&self, index: usize) -> Option<&DepCheck> {
        self.checks.get(index)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub roots: Vec<PathBuf>,
    pub config_path: Option<PathBuf>,
}

pub fn probe(lang: Lang, p: &Probe) -> DepReport {
    let t = |k: Key| i18n::t(lang, k);
    let mut checks = Vec::new();

    let version = git::version_blocking();
    checks.push(match &version {
        None => DepCheck { label: t(Key::DepGit), detail: t(Key::DepGitMissing).into(), status: Status::Error },
        Some(v) if parse_version(v).is_some_and(|ver| ver < RECOMMENDED) => DepCheck {
            label: t(Key::DepGit),
            detail: i18n::fill(t(Key::DepGitOld), &[v]),
            status: Status::Warning,
        },
        Some(v) => DepCheck { label: t(Key::DepGit), detail: i18n::fill(t(Key::DepGitOk), &[v]), status: Status::Ok },
    });

    if version.is_some() {
        let name = git::global_config_blocking("user.name");
        let email = git::global_config_blocking("user.email");
        checks.push(match (name, email) {
            (Some(n), Some(e)) => DepCheck { label: t(Key::DepIdentity), detail: format!("{n} <{e}>"), status: Status::Ok },
            _ => DepCheck { label: t(Key::DepIdentity), detail: t(Key::DepIdentityMissing).into(), status: Status::Warning },
        });

        checks.push(match git::global_config_blocking("credential.helper") {
            Some(h) => DepCheck { label: t(Key::DepCredential), detail: h, status: Status::Ok },
            None => DepCheck { label: t(Key::DepCredential), detail: t(Key::DepCredentialNone).into(), status: Status::Info },
        });
    }

    checks.push(match editor() {
        Some(e) => DepCheck { label: t(Key::DepEditor), detail: e, status: Status::Ok },
        None => DepCheck { label: t(Key::DepEditor), detail: t(Key::DepEditorNone).into(), status: Status::Warning },
    });

    checks.push(if std::env::var_os("SSH_AUTH_SOCK").is_some() {
        DepCheck { label: t(Key::DepSsh), detail: t(Key::DepSshOk).into(), status: Status::Ok }
    } else {
        DepCheck { label: t(Key::DepSsh), detail: t(Key::DepSshNone).into(), status: Status::Info }
    });

    checks.push(match clipboard_tool() {
        Some(c) => DepCheck { label: t(Key::DepClipboard), detail: c.to_string(), status: Status::Ok },
        None => DepCheck { label: t(Key::DepClipboard), detail: t(Key::DepClipboardNone).into(), status: Status::Info },
    });

    let missing: Vec<String> = p.roots.iter().filter(|r| !r.is_dir()).map(|r| r.display().to_string()).collect();
    checks.push(if missing.is_empty() {
        let list = p.roots.iter().map(|r| crate::config::shorten_home(r)).collect::<Vec<_>>().join(", ");
        DepCheck { label: t(Key::DepRoots), detail: list, status: Status::Ok }
    } else {
        DepCheck { label: t(Key::DepRoots), detail: i18n::fill(t(Key::DepRootsMissing), &[&missing.join(", ")]), status: Status::Warning }
    });

    checks.push(DepCheck {
        label: t(Key::DepConfig),
        detail: p.config_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t(Key::DepConfigDefault).into()),
        status: Status::Info,
    });

    DepReport { checks }
}

fn parse_version(v: &str) -> Option<(u32, u32)> {
    let mut it = v.split(|c: char| !c.is_ascii_digit());
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// Editor que usaría `git commit`: mismas prioridades que git.
pub fn editor() -> Option<String> {
    ["GIT_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
        .or_else(|| git::global_config_blocking("core.editor"))
}

pub fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(bin)).find(|p| is_executable(p))
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        p.metadata().map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// Herramienta para copiar al portapapeles, con sus argumentos.
pub fn clipboard_tool() -> Option<&'static str> {
    ["pbcopy", "wl-copy", "xclip", "xsel"].into_iter().find(|b| which(b).is_some())
}

pub fn clipboard_args(tool: &str) -> &'static [&'static str] {
    match tool {
        "xclip" => &["-selection", "clipboard"],
        "xsel" => &["--clipboard", "--input"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version() {
        assert_eq!(parse_version("2.39.3 (Apple Git-146)"), Some((2, 39)));
        assert!(parse_version("2.30.1").unwrap() < RECOMMENDED);
        assert_eq!(parse_version("x"), None);
    }
}
