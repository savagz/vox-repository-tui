//! Escrituras: cada acción de la TUI se describe como un `Op` y corre en una
//! tarea aparte. El resultado (salida o error de git) vuelve como evento.

use std::path::PathBuf;

use super::model::RepoCtx;
use super::patch::{self, Apply};
use super::rebase::{self, Item, QuickEdit};
use super::{ExecOpts, Git, GitResult, sh_quote};

#[derive(Debug, Clone)]
pub enum Op {
    /// Un comando git local.
    Git(Vec<String>),
    /// Un comando git de red (timeout largo, salida de stderr).
    Net(Vec<String>),
    /// Varios pasos; se corta en el primero que falla.
    Seq(Vec<Op>),
    ApplyPatch { patch: String, cached: bool, reverse: bool },
    /// Indexa líneas de un archivo sin seguir: `add -N` y después el parche.
    StageNewFileLines { path: String, hunk: usize, lines: Option<Vec<usize>> },
    /// Rebase interactivo con un plan ya armado (más viejo primero).
    Rebase { base: Option<String>, items: Vec<Item> },
    QuickRebase(QuickEdit),
    /// Mete lo indexado dentro de un commit anterior (fixup + autosquash).
    AmendInto { hash: String },
    WriteFile { path: PathBuf, contents: String },
    AppendLine { path: PathBuf, line: String },
}

pub fn git<I, S>(args: I) -> Op
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Op::Git(args.into_iter().map(Into::into).collect())
}

pub fn net<I, S>(args: I) -> Op
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Op::Net(args.into_iter().map(Into::into).collect())
}

pub async fn execute(git: &Git, ctx: &RepoCtx, op: Op) -> GitResult<String> {
    match op {
        Op::Git(args) => git.run(&args).await,
        Op::Net(args) => git.run_net(&args).await,
        Op::Seq(ops) => {
            let mut out = String::new();
            for op in ops {
                let text = Box::pin(execute(git, ctx, op)).await?;
                out.push_str(&text);
            }
            Ok(out)
        }
        Op::ApplyPatch { patch, cached, reverse } => apply(git, patch, cached, reverse).await,
        Op::StageNewFileLines { path, hunk, lines } => {
            git.run(&["add", "--intent-to-add", "--", &path]).await?;
            let text = git.run(&["diff", "--", &path]).await?;
            let diff = patch::parse(&text);
            let Some(p) = patch::build(&diff, hunk, lines.as_deref(), Apply::Forward) else {
                return Err("empty patch".into());
            };
            apply(git, p, true, false).await
        }
        Op::Rebase { base, items } => run_rebase(git, ctx, base.as_deref(), &items).await,
        Op::QuickRebase(edit) => quick_rebase(git, ctx, &edit).await,
        Op::AmendInto { hash } => {
            git.run(&["commit", "--fixup", &hash]).await?;
            let base = parent_of(git, &hash).await;
            let mut args = vec!["rebase", "-i", "--autosquash", "--autostash"];
            match &base {
                Some(b) => args.push(b),
                None => args.push("--root"),
            }
            let opts = ExecOpts { env: vec![("GIT_SEQUENCE_EDITOR".into(), "true".into())], ..ExecOpts::default() };
            git.exec(&args, opts).await
        }
        Op::WriteFile { path, contents } => {
            let full = ctx.root.join(path);
            tokio::fs::write(&full, contents).await.map_err(|e| format!("{}: {e}", full.display()))?;
            Ok(String::new())
        }
        Op::AppendLine { path, line } => {
            let full = ctx.root.join(path);
            let mut current = tokio::fs::read_to_string(&full).await.unwrap_or_default();
            if !current.is_empty() && !current.ends_with('\n') {
                current.push('\n');
            }
            current.push_str(&line);
            current.push('\n');
            tokio::fs::write(&full, current).await.map_err(|e| format!("{}: {e}", full.display()))?;
            Ok(String::new())
        }
    }
}

async fn apply(git: &Git, patch: String, cached: bool, reverse: bool) -> GitResult<String> {
    let mut args = vec!["apply", "--whitespace=nowarn"];
    if cached {
        args.push("--cached");
    }
    if reverse {
        args.push("--reverse");
    }
    args.push("-");
    git.exec(&args, ExecOpts::stdin(patch)).await
}

/// `hash^` si existe; `None` si es el commit raíz.
async fn parent_of(git: &Git, hash: &str) -> Option<String> {
    ancestor(git, hash, 1).await
}

async fn ancestor(git: &Git, hash: &str, n: usize) -> Option<String> {
    let spec = format!("{hash}~{n}");
    git.run(&["rev-parse", "--verify", "-q", &spec]).await.ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

async fn quick_rebase(git: &Git, ctx: &RepoCtx, edit: &QuickEdit) -> GitResult<String> {
    let base = ancestor(git, edit.hash(), edit.depth()).await;
    let range = match &base {
        Some(b) => format!("{b}..HEAD"),
        None => "HEAD".to_string(),
    };
    let merges = git.run(&["rev-list", "--merges", "--count", &range]).await?;
    if merges.trim() != "0" {
        return Err("the range contains merge commits; use git rebase --rebase-merges manually".into());
    }
    let raw = git.run(&["log", "--reverse", "--format=%H%x1f%s", &range, "--"]).await?;
    let mut items: Vec<Item> = raw
        .lines()
        .filter_map(|l| l.split_once('\x1f').map(|(h, s)| Item::pick(h, s)))
        .collect();
    edit.apply(&mut items).ok_or_else(|| "the commit cannot be moved or melded there".to_string())?;
    run_rebase(git, ctx, base.as_deref(), &items).await
}

async fn run_rebase(git: &Git, ctx: &RepoCtx, base: Option<&str>, items: &[Item]) -> GitResult<String> {
    rebase::validate(items).map_err(|e| format!("invalid rebase plan: {e:?}"))?;
    let dir = ctx.git_dir.join("voxrepository");
    let _ = tokio::fs::remove_dir_all(&dir).await;
    tokio::fs::create_dir_all(&dir).await.map_err(|e| format!("{}: {e}", dir.display()))?;

    for (i, item) in items.iter().enumerate() {
        if let Some(msg) = &item.message {
            let path = dir.join(format!("msg-{i}.txt"));
            tokio::fs::write(&path, msg).await.map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    let todo = rebase::todo(items, |i| dir.join(format!("msg-{i}.txt")).display().to_string());
    let todo_path = dir.join("todo");
    tokio::fs::write(&todo_path, todo).await.map_err(|e| format!("{}: {e}", todo_path.display()))?;

    let editor = format!("cp {}", sh_quote(&todo_path.display().to_string()));
    let mut args = vec!["rebase", "-i", "--autostash"];
    match base {
        Some(b) => args.push(b),
        None => args.push("--root"),
    }
    let opts = ExecOpts { env: vec![("GIT_SEQUENCE_EDITOR".into(), editor)], ..ExecOpts::default() };
    git.exec(&args, opts).await
}
