//! Lecturas: snapshot completo de un repo, polling de estado, resumen para
//! el workspace y textos de detalle (diffs, commits, stashes...).

use std::path::{Path, PathBuf};

use super::model::*;
use super::{ExecOpts, Git, GitResult, parse};

pub const ACTIVITY_DAYS: usize = 30;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogOpts {
    pub limit: usize,
    /// Ref a mostrar en vez de HEAD (al inspeccionar otra rama).
    pub reference: Option<String>,
    pub all: bool,
}

/// Resuelve raíz y directorio git. Falla en repos bare (no hay árbol).
pub async fn open(git: &Git) -> GitResult<RepoCtx> {
    let out = git.run(&["rev-parse", "--show-toplevel", "--absolute-git-dir"]).await?;
    let mut lines = out.lines();
    let (Some(root), Some(git_dir)) = (lines.next(), lines.next()) else {
        return Err("git rev-parse: unexpected output".into());
    };
    let root = PathBuf::from(root);
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.display().to_string());
    Ok(RepoCtx { root, git_dir: PathBuf::from(git_dir), name })
}

pub async fn status(git: &Git) -> GitResult<Status> {
    let raw = git.run(&["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=all"]).await?;
    Ok(parse::status(&raw))
}

pub async fn log(git: &Git, opts: &LogOpts) -> GitResult<Vec<LogRow>> {
    let limit = opts.limit.max(1).to_string();
    let mut args: Vec<&str> = vec!["log", "--graph", "--date-order", parse::LOG_FORMAT, "-n", &limit];
    if opts.all {
        args.push("--all");
    } else if let Some(r) = &opts.reference {
        args.push(r);
    } else {
        args.push("HEAD");
    }
    args.push("--");
    Ok(parse::log(&git.run(&args).await?))
}

pub async fn snapshot(git: &Git, ctx: &RepoCtx, log_opts: &LogOpts) -> RepoSnapshot {
    let since = format!("--since={ACTIVITY_DAYS}.days");
    let activity_args = ["log", since.as_str(), "--format=%at%x1f%an", "HEAD", "--"];
    let (status_r, log_r, branches_r, remotes_r, stashes_r, tags_r, worktrees_r, reflog_r, activity_r) = tokio::join!(
        status(git),
        log(git, log_opts),
        git.run(&["for-each-ref", parse::BRANCH_FORMAT, "refs/heads", "refs/remotes"]),
        git.run(&["remote", "-v"]),
        git.run(&["stash", "list", parse::STASH_FORMAT]),
        git.run(&["for-each-ref", "--sort=-creatordate", parse::TAG_FORMAT, "refs/tags"]),
        git.run(&["worktree", "list", "--porcelain"]),
        git.run(&["reflog", parse::REFLOG_FORMAT, "-n", "300"]),
        git.run(&activity_args),
    );

    let mut snap = RepoSnapshot { op: OpState::detect(&ctx.git_dir), ..RepoSnapshot::default() };
    let mut errors = Vec::new();
    match status_r {
        Ok(s) => snap.status = s,
        Err(e) => errors.push(e),
    }
    let born = snap.status.head.oid.is_some();
    match log_r {
        Ok(l) => snap.log = l,
        Err(e) if born || log_opts.reference.is_some() => errors.push(e),
        Err(_) => {}
    }
    match branches_r {
        Ok(raw) => snap.branches = parse::branches(&raw),
        Err(e) => errors.push(e),
    }
    match remotes_r {
        Ok(raw) => snap.remotes = parse::remotes(&raw),
        Err(e) => errors.push(e),
    }
    if let Ok(raw) = stashes_r {
        snap.stashes = parse::stashes(&raw);
    }
    match tags_r {
        Ok(raw) => snap.tags = parse::tags(&raw),
        Err(e) => errors.push(e),
    }
    if let Ok(raw) = worktrees_r {
        snap.worktrees = parse::worktrees(&raw);
    }
    if let Ok(raw) = reflog_r {
        snap.reflog = parse::reflog(&raw);
    }
    if let Ok(raw) = activity_r {
        let (buckets, authors) = parse::activity(&raw, chrono::Utc::now().timestamp(), ACTIVITY_DAYS);
        snap.activity = buckets;
        snap.authors = authors;
    } else {
        snap.activity = vec![0; ACTIVITY_DAYS];
    }
    snap.errors = errors;
    snap
}

/// Resumen de una fila del workspace.
pub async fn summary(git: &Git) -> GitResult<RepoSummary> {
    let (st, last, origin, stashes, git_dir) = tokio::join!(
        status(git),
        git.run(&["log", "-1", "--format=%at%x1f%s", "HEAD", "--"]),
        git.run(&["remote", "get-url", "origin"]),
        git.run(&["stash", "list", "--format=%gd"]),
        git.run(&["rev-parse", "--absolute-git-dir"]),
    );
    let st = st?;
    let (last_time, last_subject) = match last {
        Ok(raw) => match raw.trim_end().split_once('\x1f') {
            Some((ts, s)) => (ts.parse().ok(), s.to_string()),
            None => (None, String::new()),
        },
        Err(_) => (None, String::new()),
    };
    let op = git_dir.map(|d| OpState::detect(Path::new(d.trim()))).unwrap_or_default();
    Ok(RepoSummary {
        counts: st.counts(),
        head: st.head,
        last_time,
        last_subject,
        origin: origin.ok().map(|o| o.trim().to_string()).filter(|o| !o.is_empty()),
        stashes: stashes.map(|s| s.lines().count()).unwrap_or(0),
        op,
    })
}

/// Diff de un archivo del árbol: sin seguir (contra /dev/null), en el índice
/// (`staged`) o sin indexar.
pub async fn file_diff(git: &Git, path: &str, staged: bool, untracked: bool) -> GitResult<String> {
    if untracked {
        let opts = ExecOpts { ok_codes: vec![1], ..ExecOpts::default() };
        return git.exec(&["diff", "--no-index", "--", "/dev/null", path], opts).await;
    }
    if staged {
        git.run(&["diff", "--cached", "--", path]).await
    } else {
        git.run(&["diff", "--", path]).await
    }
}

const SHOW_FORMAT: &str = "--format=commit %H%nAuthor: %an <%ae>%nDate:   %ad%nRefs:   %D%n%n%w(0,4,4)%B";

pub async fn commit_show(git: &Git, hash: &str) -> GitResult<String> {
    git.run(&["show", "--stat", "--patch", "--date=iso", SHOW_FORMAT, hash, "--"]).await
}

pub async fn commit_files(git: &Git, hash: &str, merge: bool) -> GitResult<Vec<ChangedFile>> {
    let raw = if merge {
        let parent = format!("{hash}^1");
        git.run(&["diff", "--name-status", "-z", "-M", &parent, hash]).await?
    } else {
        git.run(&["diff-tree", "--no-commit-id", "-r", "-z", "--name-status", "-M", "--root", hash]).await?
    };
    Ok(parse::name_status(&raw))
}

pub async fn commit_file_diff(git: &Git, hash: &str, path: &str, merge: bool) -> GitResult<String> {
    if merge {
        let parent = format!("{hash}^1");
        git.run(&["diff", "-M", &parent, hash, "--", path]).await
    } else {
        git.run(&["show", "--format=", "-M", hash, "--", path]).await
    }
}

pub async fn stash_show(git: &Git, selector: &str) -> GitResult<String> {
    match git.run(&["stash", "show", "--stat", "-p", "--include-untracked", selector]).await {
        Ok(t) => Ok(t),
        // `--include-untracked` en `stash show` existe desde git 2.32.
        Err(_) => git.run(&["stash", "show", "--stat", "-p", selector]).await,
    }
}

pub async fn ref_show(git: &Git, reference: &str) -> GitResult<String> {
    git.run(&["show", "--stat", "--date=iso", SHOW_FORMAT, reference, "--"]).await
}

pub async fn branch_log(git: &Git, reference: &str) -> GitResult<String> {
    git.run(&["log", "--graph", "--format=%h %cs %s  (%an)%d", "-n", "80", reference, "--"]).await
}

pub async fn remote_show(git: &Git, name: &str) -> GitResult<String> {
    git.run(&["remote", "show", "-n", name]).await
}

pub async fn worktree_status(git: &Git, path: &str) -> GitResult<String> {
    git.at(Path::new(path)).run(&["status", "--short", "--branch"]).await
}

/// Recorre las raíces buscando repos (un directorio con `.git`, carpeta o
/// archivo). No desciende dentro de un repo encontrado ni en directorios
/// ocultos o ignorados. Bloqueante: correr en `spawn_blocking`.
pub fn scan(roots: &[PathBuf], max_depth: usize, ignore: &[String]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in roots {
        walk(root, 0, max_depth, ignore, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

fn walk(dir: &Path, depth: usize, max_depth: usize, ignore: &[String], found: &mut Vec<PathBuf>) {
    if dir.join(".git").exists() {
        found.push(dir.to_path_buf());
        return;
    }
    if depth >= max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            !name.starts_with('.') && !ignore.iter().any(|i| i == &name)
        })
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for d in dirs {
        walk(&d, depth + 1, max_depth, ignore, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_encuentra_repos_y_no_entra_en_ellos() {
        let base = std::env::temp_dir().join(format!("voxrepo-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("a/.git")).unwrap();
        std::fs::create_dir_all(base.join("a/sub/.git")).unwrap();
        std::fs::create_dir_all(base.join("grupo/b/.git")).unwrap();
        std::fs::create_dir_all(base.join("node_modules/c/.git")).unwrap();
        std::fs::create_dir_all(base.join(".oculto/d/.git")).unwrap();
        std::fs::create_dir_all(base.join("x/y/z/hondo/.git")).unwrap();
        let found = scan(std::slice::from_ref(&base), 2, &["node_modules".to_string()]);
        let names: Vec<_> = found.iter().map(|p| p.strip_prefix(&base).unwrap().display().to_string()).collect();
        assert_eq!(names, vec!["a", "grupo/b"]);
        let _ = std::fs::remove_dir_all(&base);
    }
}
