//! Pruebas contra repos git reales en un directorio temporal: verifican que
//! los formatos, los parches parciales y los rebases funcionan con el git
//! instalado, no sólo con salidas inventadas.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::load::{self, LogOpts};
use super::ops::{self, Op};
use super::patch::{self, Apply};
use super::rebase::{Action, Item, QuickEdit};
use super::{Git, conflict};

struct Repo {
    dir: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn sh(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_EDITOR", "true")
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

impl Repo {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("voxrepo-it-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = Self { dir: dir.canonicalize().unwrap() };
        sh(&r.dir, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.name", "Test"), ("user.email", "t@example.com"), ("commit.gpgsign", "false"), ("tag.gpgsign", "false")] {
            sh(&r.dir, &["config", k, v]);
        }
        r
    }

    fn write(&self, path: &str, content: &str) {
        std::fs::write(self.dir.join(path), content).unwrap();
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.join(path)).unwrap()
    }

    fn commit(&self, path: &str, content: &str, msg: &str) {
        self.write(path, content);
        sh(&self.dir, &["add", path]);
        sh(&self.dir, &["commit", "-q", "-m", msg]);
    }

    fn git(&self) -> Git {
        Git::new(&self.dir, 20, 20)
    }

    fn subjects(&self) -> Vec<String> {
        sh(&self.dir, &["log", "--format=%s"]).lines().map(str::to_string).collect()
    }
}

const BASE: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n";

#[tokio::test]
async fn snapshot_lee_todo() {
    let r = Repo::new("snap");
    r.commit("a.txt", "a\n", "primero");
    r.commit("b.txt", "b\n", "segundo");
    sh(&r.dir, &["branch", "dev"]);
    sh(&r.dir, &["tag", "-a", "v1", "-m", "release uno"]);
    sh(&r.dir, &["remote", "add", "origin", "https://example.com/x.git"]);
    r.write("a.txt", "cambio\n");
    sh(&r.dir, &["stash", "push", "-q", "-m", "guardado"]);
    r.write("nuevo con espacio.txt", "n\n");
    r.write("b.txt", "b2\n");

    let git = r.git();
    let ctx = load::open(&git).await.unwrap();
    assert_eq!(ctx.root, r.dir);
    let snap = load::snapshot(&git, &ctx, &LogOpts { limit: 50, ..LogOpts::default() }).await;
    assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    assert_eq!(snap.status.head.head.as_deref(), Some("main"));
    let c = snap.status.counts();
    assert_eq!((c.unstaged, c.untracked), (1, 1));
    assert!(snap.status.entries.iter().any(|e| e.path == "nuevo con espacio.txt"));
    let commits: Vec<_> = snap.log.iter().filter_map(|r| r.commit.as_ref()).collect();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].subject, "segundo");
    assert!(commits[0].refs.contains("HEAD -> main"));
    assert_eq!(snap.branches.iter().filter(|b| !b.is_remote).count(), 2);
    assert!(snap.branches[0].is_head);
    assert_eq!(snap.tags.len(), 1);
    assert!(snap.tags[0].annotated);
    assert_eq!(snap.remotes[0].name, "origin");
    assert_eq!(snap.stashes.len(), 1);
    assert!(snap.stashes[0].message.contains("guardado"));
    assert_eq!(snap.worktrees.len(), 1);
    assert!(!snap.reflog.is_empty());
    assert_eq!(snap.activity.iter().sum::<u32>(), 2);

    let summary = load::summary(&git).await.unwrap();
    assert_eq!(summary.stashes, 1);
    assert_eq!(summary.last_subject, "segundo");
    assert_eq!(summary.origin.as_deref(), Some("https://example.com/x.git"));
}

#[tokio::test]
async fn indexa_y_desindexa_lineas() {
    let r = Repo::new("lines");
    r.commit("f.txt", BASE, "base");
    r.write("f.txt", &BASE.replace("2\n", "dos\n").replace("14\n", "catorce\n"));
    let git = r.git();
    let ctx = load::open(&git).await.unwrap();

    let diff = patch::parse(&load::file_diff(&git, "f.txt", false, false).await.unwrap());
    assert_eq!(diff.hunks.len(), 2);
    // Sólo el '+dos' del primer hunk: "-2" queda como contexto.
    let plus = diff.hunks[0].lines.iter().position(|l| l == "+dos").unwrap();
    let p = patch::build(&diff, 0, Some(&[plus]), Apply::Forward).unwrap();
    ops::execute(&git, &ctx, Op::ApplyPatch { patch: p, cached: true, reverse: false }).await.unwrap();
    let cached = sh(&r.dir, &["diff", "--cached"]);
    assert!(cached.contains("+dos") && !cached.contains("-2\n") && !cached.contains("catorce"), "{cached}");

    // Indexar el segundo hunk entero y luego desindexar sólo su '+'.
    let diff = patch::parse(&load::file_diff(&git, "f.txt", false, false).await.unwrap());
    let hunk = diff.hunks.iter().position(|h| h.lines.iter().any(|l| l == "+catorce")).unwrap();
    let p = patch::build(&diff, hunk, None, Apply::Forward).unwrap();
    ops::execute(&git, &ctx, Op::ApplyPatch { patch: p, cached: true, reverse: false }).await.unwrap();
    let staged = patch::parse(&load::file_diff(&git, "f.txt", true, false).await.unwrap());
    let h = staged.hunks.iter().position(|h| h.lines.iter().any(|l| l == "+catorce")).unwrap();
    let plus = staged.hunks[h].lines.iter().position(|l| l == "+catorce").unwrap();
    let p = patch::build(&staged, h, Some(&[plus]), Apply::Reverse).unwrap();
    ops::execute(&git, &ctx, Op::ApplyPatch { patch: p, cached: true, reverse: true }).await.unwrap();
    let cached = sh(&r.dir, &["diff", "--cached"]);
    assert!(cached.contains("-14") && !cached.contains("+catorce"), "{cached}");
    // El árbol de trabajo no se tocó.
    assert!(r.read("f.txt").contains("catorce"));
}

#[tokio::test]
async fn descarta_lineas_del_arbol() {
    let r = Repo::new("discard");
    r.commit("f.txt", BASE, "base");
    r.write("f.txt", &BASE.replace("3\n", "tres\n").replace("4\n", "cuatro\n"));
    let git = r.git();
    let ctx = load::open(&git).await.unwrap();
    let diff = patch::parse(&load::file_diff(&git, "f.txt", false, false).await.unwrap());
    let lines: Vec<usize> = diff.hunks[0].lines.iter().enumerate().filter(|(_, l)| *l == "-3" || *l == "+tres").map(|(i, _)| i).collect();
    let p = patch::build(&diff, 0, Some(&lines), Apply::Reverse).unwrap();
    ops::execute(&git, &ctx, Op::ApplyPatch { patch: p, cached: false, reverse: true }).await.unwrap();
    let text = r.read("f.txt");
    assert!(text.contains("\n3\n") && text.contains("cuatro"), "{text}");
}

#[tokio::test]
async fn indexa_lineas_de_archivo_nuevo() {
    let r = Repo::new("newfile");
    r.commit("base.txt", "x\n", "base");
    r.write("nuevo.txt", "a\nb\nc\n");
    let git = r.git();
    let ctx = load::open(&git).await.unwrap();
    let text = load::file_diff(&git, "nuevo.txt", false, true).await.unwrap();
    let diff = patch::parse(&text);
    let b = diff.hunks[0].lines.iter().position(|l| l == "+b").unwrap();
    ops::execute(&git, &ctx, Op::StageNewFileLines { path: "nuevo.txt".into(), hunk: 0, lines: Some(vec![b]) }).await.unwrap();
    assert_eq!(sh(&r.dir, &["show", ":nuevo.txt"]), "b\n");
}

#[tokio::test]
async fn ediciones_rapidas_de_historia() {
    let r = Repo::new("quick");
    r.commit("a.txt", "1\n", "uno");
    r.commit("b.txt", "2\n", "dos");
    r.commit("c.txt", "3\n", "tres");
    r.commit("d.txt", "4\n", "cuatro");
    let git = r.git();
    let ctx = load::open(&git).await.unwrap();
    let hash = |subject: &str| sh(&r.dir, &["log", "--format=%H", &format!("--grep=^{subject}$")]).trim().to_string();

    ops::execute(&git, &ctx, Op::QuickRebase(QuickEdit::Reword { hash: hash("dos"), message: "DOS\n\ncuerpo".into() })).await.unwrap();
    assert_eq!(r.subjects(), vec!["cuatro", "tres", "DOS", "uno"]);
    assert!(sh(&r.dir, &["log", "--format=%B", "-1", "HEAD~2"]).contains("cuerpo"));

    ops::execute(&git, &ctx, Op::QuickRebase(QuickEdit::MoveUp { hash: hash("DOS") })).await.unwrap();
    assert_eq!(r.subjects(), vec!["cuatro", "DOS", "tres", "uno"]);

    ops::execute(&git, &ctx, Op::QuickRebase(QuickEdit::Fixup { hash: hash("cuatro") })).await.unwrap();
    assert_eq!(r.subjects(), vec!["DOS", "tres", "uno"]);
    assert!(sh(&r.dir, &["show", "--stat", "--format=", "HEAD"]).contains("d.txt"));

    ops::execute(&git, &ctx, Op::QuickRebase(QuickEdit::Drop { hash: hash("tres") })).await.unwrap();
    assert_eq!(r.subjects(), vec!["DOS", "uno"]);
    assert!(!r.dir.join("c.txt").exists());

    // Sobre el commit raíz: reword usa --root.
    ops::execute(&git, &ctx, Op::QuickRebase(QuickEdit::Reword { hash: hash("uno"), message: "UNO".into() })).await.unwrap();
    assert_eq!(r.subjects(), vec!["DOS", "UNO"]);
    // No se puede fusionar el commit raíz con nada.
    assert!(ops::execute(&git, &ctx, Op::QuickRebase(QuickEdit::Squash { hash: hash("UNO") })).await.is_err());
}

#[tokio::test]
async fn planificador_y_amend_into() {
    let r = Repo::new("plan");
    r.commit("a.txt", "1\n", "uno");
    r.commit("b.txt", "2\n", "dos");
    r.commit("c.txt", "3\n", "tres");
    let git = r.git();
    let ctx = load::open(&git).await.unwrap();
    let log: Vec<(String, String)> = sh(&r.dir, &["log", "--reverse", "--format=%H %s"])
        .lines()
        .map(|l| {
            let (h, s) = l.split_once(' ').unwrap();
            (h.to_string(), s.to_string())
        })
        .collect();
    let base = log[0].0.clone();
    let mut items: Vec<Item> = log[1..].iter().map(|(h, s)| Item::pick(h, s)).collect();
    items.swap(0, 1);
    items[0].action = Action::Reword;
    items[0].message = Some("TRES".into());
    items[1].action = Action::Squash;
    ops::execute(&git, &ctx, Op::Rebase { base: Some(base), items }).await.unwrap();
    assert_eq!(r.subjects(), vec!["TRES", "uno"]);

    // Meter un cambio indexado dentro del commit "uno" (no HEAD).
    let uno = sh(&r.dir, &["log", "--format=%H", "--grep=^uno$"]).trim().to_string();
    r.write("a.txt", "1 corregido\n");
    sh(&r.dir, &["add", "a.txt"]);
    ops::execute(&git, &ctx, Op::AmendInto { hash: uno }).await.unwrap();
    assert_eq!(r.subjects(), vec!["TRES", "uno"]);
    assert_eq!(sh(&r.dir, &["show", "HEAD~1:a.txt"]), "1 corregido\n");
    assert!(sh(&r.dir, &["status", "--porcelain"]).is_empty());
}

#[tokio::test]
async fn conflicto_detectado_y_resuelto() {
    let r = Repo::new("conflict");
    r.commit("f.txt", "base\n", "base");
    sh(&r.dir, &["checkout", "-q", "-b", "feat"]);
    r.commit("f.txt", "de feat\n", "feat");
    sh(&r.dir, &["checkout", "-q", "main"]);
    r.commit("f.txt", "de main\n", "main");
    let git = r.git();
    let ctx = load::open(&git).await.unwrap();
    assert!(ops::execute(&git, &ctx, ops::git(["merge", "--no-edit", "feat"])).await.is_err());

    let snap = load::snapshot(&git, &ctx, &LogOpts { limit: 10, ..LogOpts::default() }).await;
    assert_eq!(snap.op, super::model::OpState::Merge);
    assert_eq!(snap.status.counts().conflicted, 1);

    let mut segs = conflict::parse(&r.read("f.txt"));
    conflict::block_mut(&mut segs, 0).unwrap().choice = conflict::Choice::Both;
    let contents = conflict::render(&segs);
    ops::execute(&git, &ctx, Op::Seq(vec![Op::WriteFile { path: "f.txt".into(), contents }, ops::git(["add", "--", "f.txt"])])).await.unwrap();
    ops::execute(&git, &ctx, ops::git(["merge", "--continue"])).await.unwrap();
    assert_eq!(r.read("f.txt"), "de main\nde feat\n");
    assert_eq!(super::model::OpState::detect(&ctx.git_dir), super::model::OpState::None);
}

#[tokio::test]
async fn commit_files_y_show() {
    let r = Repo::new("show");
    r.commit("a.txt", "1\n", "uno");
    sh(&r.dir, &["mv", "a.txt", "b.txt"]);
    r.write("c.txt", "c\n");
    sh(&r.dir, &["add", "c.txt"]);
    sh(&r.dir, &["commit", "-q", "-m", "mover"]);
    let git = r.git();
    let head = sh(&r.dir, &["rev-parse", "HEAD"]).trim().to_string();
    let files = load::commit_files(&git, &head, false).await.unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|f| f.status == 'R' && f.orig_path.as_deref() == Some("a.txt")));
    let root = sh(&r.dir, &["rev-parse", "HEAD~1"]).trim().to_string();
    assert_eq!(load::commit_files(&git, &root, false).await.unwrap().len(), 1);
    let show = load::commit_show(&git, &head).await.unwrap();
    assert!(show.contains("mover") && show.contains("b.txt"));
}
