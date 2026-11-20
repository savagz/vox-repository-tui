//! Parsers de la salida de git. Los formatos se definen acá, junto a quien
//! los lee, para que comando y parser no se desincronicen.

use super::model::*;

const SEP: char = '\x1f';

pub const LOG_FORMAT: &str = "--format=%x1f%H%x1f%h%x1f%P%x1f%an%x1f%at%x1f%D%x1f%s";
pub const BRANCH_FORMAT: &str = "--format=%(HEAD)%1f%(refname:short)%1f%(refname)%1f%(upstream:short)%1f%(upstream:track)%1f%(objectname:short)%1f%(committerdate:unix)%1f%(contents:subject)";
pub const STASH_FORMAT: &str = "--format=%gd%x1f%ct%x1f%gs";
pub const TAG_FORMAT: &str = "--format=%(refname:short)%1f%(objectname:short)%1f%(objecttype)%1f%(creatordate:unix)%1f%(contents:subject)";
pub const REFLOG_FORMAT: &str = "--format=%h%x1f%gd%x1f%gs%x1f%ct";

/// `git status --porcelain=v2 --branch -z`.
pub fn status(raw: &str) -> Status {
    let mut st = Status::default();
    let mut records = raw.split('\0').filter(|r| !r.is_empty());
    while let Some(rec) = records.next() {
        if let Some(h) = rec.strip_prefix("# ") {
            let (key, val) = h.split_once(' ').unwrap_or((h, ""));
            match key {
                "branch.oid" => st.head.oid = (val != "(initial)").then(|| val.to_string()),
                "branch.head" => st.head.head = (val != "(detached)").then(|| val.to_string()),
                "branch.upstream" => st.head.upstream = Some(val.to_string()),
                "branch.ab" => {
                    for part in val.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            st.head.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            st.head.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = rec.as_bytes()[0];
        match kind {
            b'1' => {
                let f: Vec<&str> = rec.splitn(9, ' ').collect();
                if f.len() == 9 {
                    st.entries.push(entry(f[1], f[8], None, EntryKind::Ordinary));
                }
            }
            b'2' => {
                let f: Vec<&str> = rec.splitn(10, ' ').collect();
                let orig = records.next().map(str::to_string);
                if f.len() == 10 {
                    st.entries.push(entry(f[1], f[9], orig, EntryKind::Renamed));
                }
            }
            b'u' => {
                let f: Vec<&str> = rec.splitn(11, ' ').collect();
                if f.len() == 11 {
                    st.entries.push(entry(f[1], f[10], None, EntryKind::Unmerged));
                }
            }
            b'?' => st.entries.push(StatusEntry {
                path: rec[2..].to_string(),
                orig_path: None,
                index: '?',
                worktree: '?',
                kind: EntryKind::Untracked,
            }),
            _ => {}
        }
    }
    st
}

fn entry(xy: &str, path: &str, orig: Option<String>, kind: EntryKind) -> StatusEntry {
    let mut c = xy.chars();
    StatusEntry {
        path: path.to_string(),
        orig_path: orig,
        index: c.next().unwrap_or('.'),
        worktree: c.next().unwrap_or('.'),
        kind,
    }
}

/// `git log --graph` con `LOG_FORMAT`.
pub fn log(raw: &str) -> Vec<LogRow> {
    raw.lines()
        .map(|line| match line.split_once(SEP) {
            None => LogRow { graph: line.trim_end().to_string(), commit: None },
            Some((graph, rest)) => {
                let f: Vec<&str> = rest.splitn(7, SEP).collect();
                if f.len() < 7 {
                    return LogRow { graph: graph.to_string(), commit: None };
                }
                LogRow {
                    graph: graph.to_string(),
                    commit: Some(Commit {
                        hash: f[0].to_string(),
                        short: f[1].to_string(),
                        parents: f[2].split_whitespace().map(str::to_string).collect(),
                        author: f[3].to_string(),
                        time: f[4].parse().unwrap_or(0),
                        refs: f[5].to_string(),
                        subject: f[6].to_string(),
                    }),
                }
            }
        })
        .collect()
}

/// `git for-each-ref refs/heads refs/remotes` con `BRANCH_FORMAT`.
pub fn branches(raw: &str) -> Vec<Branch> {
    let mut out: Vec<Branch> = raw
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(8, SEP).collect();
            if f.len() < 8 {
                return None;
            }
            let full_ref = f[2].to_string();
            // `origin/HEAD` es un alias simbólico, no una rama.
            if full_ref.starts_with("refs/remotes/") && full_ref.ends_with("/HEAD") {
                return None;
            }
            let (ahead, behind, gone) = track(f[4]);
            Some(Branch {
                name: f[1].to_string(),
                is_remote: full_ref.starts_with("refs/remotes/"),
                full_ref,
                is_head: f[0].trim() == "*",
                upstream: (!f[3].is_empty()).then(|| f[3].to_string()),
                ahead,
                behind,
                gone,
                short: f[5].to_string(),
                time: f[6].parse().unwrap_or(0),
                subject: f[7].to_string(),
            })
        })
        .collect();
    // Locales primero (HEAD arriba), después remotas.
    out.sort_by(|a, b| a.is_remote.cmp(&b.is_remote).then(b.is_head.cmp(&a.is_head)));
    out
}

/// `[ahead 1, behind 2]`, `[gone]` o vacío.
pub fn track(s: &str) -> (u32, u32, bool) {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner == "gone" {
        return (0, 0, true);
    }
    let mut ahead = 0;
    let mut behind = 0;
    for part in inner.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = n.parse().unwrap_or(0);
        }
    }
    (ahead, behind, false)
}

/// `git remote -v`.
pub fn remotes(raw: &str) -> Vec<Remote> {
    let mut out: Vec<Remote> = Vec::new();
    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(url), kind) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let idx = match out.iter().position(|r| r.name == name) {
            Some(i) => i,
            None => {
                out.push(Remote { name: name.into(), fetch_url: String::new(), push_url: String::new() });
                out.len() - 1
            }
        };
        if kind == Some("(push)") {
            out[idx].push_url = url.into();
        } else {
            out[idx].fetch_url = url.into();
        }
    }
    out
}

pub fn stashes(raw: &str) -> Vec<Stash> {
    raw.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(3, SEP).collect();
            (f.len() == 3).then(|| Stash {
                selector: f[0].to_string(),
                time: f[1].parse().unwrap_or(0),
                message: f[2].to_string(),
            })
        })
        .collect()
}

pub fn tags(raw: &str) -> Vec<Tag> {
    raw.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(5, SEP).collect();
            (f.len() == 5).then(|| Tag {
                name: f[0].to_string(),
                short: f[1].to_string(),
                annotated: f[2] == "tag",
                time: f[3].parse().unwrap_or(0),
                subject: f[4].to_string(),
            })
        })
        .collect()
}

/// `git worktree list --porcelain`.
pub fn worktrees(raw: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in raw.lines() {
        if line.is_empty() {
            if let Some(w) = cur.take() {
                out.push(w);
            }
            continue;
        }
        let (key, val) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                if let Some(w) = cur.take() {
                    out.push(w);
                }
                cur = Some(Worktree { path: val.to_string(), ..Worktree::default() });
            }
            _ => {
                let Some(w) = cur.as_mut() else { continue };
                match key {
                    "HEAD" => w.head = val.chars().take(7).collect(),
                    "branch" => w.branch = Some(val.trim_start_matches("refs/heads/").to_string()),
                    "bare" => w.bare = true,
                    "detached" => w.detached = true,
                    "locked" => w.locked = true,
                    "prunable" => w.prunable = true,
                    _ => {}
                }
            }
        }
    }
    if let Some(w) = cur {
        out.push(w);
    }
    out
}

pub fn reflog(raw: &str) -> Vec<ReflogEntry> {
    raw.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.splitn(4, SEP).collect();
            (f.len() == 4).then(|| ReflogEntry {
                short: f[0].to_string(),
                selector: f[1].to_string(),
                subject: f[2].to_string(),
                time: f[3].parse().unwrap_or(0),
            })
        })
        .collect()
}

/// `git diff-tree --name-status -z -M -r`: registros `S\0path\0` o, en
/// renombres/copias, `R100\0orig\0path\0`.
pub fn name_status(raw: &str) -> Vec<ChangedFile> {
    let mut out = Vec::new();
    let mut it = raw.split('\0').filter(|s| !s.is_empty());
    while let Some(code) = it.next() {
        let status = code.chars().next().unwrap_or('?');
        if matches!(status, 'R' | 'C') {
            let orig = it.next().map(str::to_string);
            if let Some(path) = it.next() {
                out.push(ChangedFile { status, path: path.to_string(), orig_path: orig });
            }
        } else if let Some(path) = it.next() {
            out.push(ChangedFile { status, path: path.to_string(), orig_path: None });
        }
    }
    out
}

/// Commits por día de los últimos `days` días a partir de timestamps `%at`.
pub fn activity(raw: &str, now: i64, days: usize) -> (Vec<u32>, Vec<(String, u32)>) {
    let mut buckets = vec![0u32; days];
    let mut authors: Vec<(String, u32)> = Vec::new();
    for line in raw.lines() {
        let (ts, author) = line.split_once(SEP).unwrap_or((line, ""));
        let Ok(ts) = ts.parse::<i64>() else { continue };
        let age_days = ((now - ts).max(0) / 86_400) as usize;
        if age_days < days {
            buckets[days - 1 - age_days] += 1;
        }
        if !author.is_empty() {
            match authors.iter_mut().find(|(a, _)| a == author) {
                Some((_, n)) => *n += 1,
                None => authors.push((author.to_string(), 1)),
            }
        }
    }
    authors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (buckets, authors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn status_porcelain_v2() {
        let raw = "# branch.oid abc123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa bbb src/a b.rs\0\
1 A. N... 000000 100644 100644 000 ccc new.rs\0\
2 R. N... 100644 100644 100644 ddd eee R100 nuevo.rs\0viejo.rs\0\
u UU N... 100644 100644 100644 100644 f1 f2 f3 conf.txt\0\
? sin seguir.txt\0";
        let st = status(raw);
        assert_eq!(st.head.oid.as_deref(), Some("abc123"));
        assert_eq!(st.head.head.as_deref(), Some("main"));
        assert_eq!(st.head.upstream.as_deref(), Some("origin/main"));
        assert_eq!((st.head.ahead, st.head.behind), (2, 1));
        assert_eq!(st.entries.len(), 5);
        assert_eq!(st.entries[0].path, "src/a b.rs");
        assert!(st.entries[0].has_unstaged() && !st.entries[0].has_staged());
        assert!(st.entries[1].has_staged());
        assert_eq!(st.entries[2].orig_path.as_deref(), Some("viejo.rs"));
        assert_eq!(st.entries[2].path, "nuevo.rs");
        assert!(st.entries[3].is_conflicted());
        assert_eq!(st.entries[4].path, "sin seguir.txt");
        let c = st.counts();
        assert_eq!((c.staged, c.unstaged, c.untracked, c.conflicted), (2, 1, 1, 1));
    }

    #[test]
    fn status_repo_vacio_y_detached() {
        let st = status("# branch.oid (initial)\0# branch.head (detached)\0");
        assert_eq!(st.head.oid, None);
        assert_eq!(st.head.head, None);
    }

    #[test]
    fn log_con_grafo() {
        let raw = "* \x1fH1\x1fh1\x1fP1 P2\x1fAna\x1f100\x1fHEAD -> main\x1fMerge x\n|\\  \n| * \x1fH2\x1fh2\x1fP3\x1fLuis\x1f90\x1f\x1fsujeto | con barra\n";
        let rows = log(raw);
        assert_eq!(rows.len(), 3);
        let c = rows[0].commit.as_ref().unwrap();
        assert_eq!(c.parents.len(), 2);
        assert_eq!(c.refs, "HEAD -> main");
        assert!(rows[1].commit.is_none());
        assert_eq!(rows[2].graph, "| * ");
        assert_eq!(rows[2].commit.as_ref().unwrap().subject, "sujeto | con barra");
    }

    #[test]
    fn branches_ordena_y_descarta_head_remoto() {
        let raw = " \x1fdev\x1frefs/heads/dev\x1f\x1f\x1fa1\x1f10\x1fs1\n\
* \x1fmain\x1frefs/heads/main\x1forigin/main\x1f[ahead 1, behind 3]\x1fa2\x1f20\x1fs2\n\
 \x1forigin\x1frefs/remotes/origin/HEAD\x1f\x1f\x1fa2\x1f20\x1fs2\n\
 \x1forigin/main\x1frefs/remotes/origin/main\x1f\x1f\x1fa2\x1f20\x1fs2\n";
        let b = branches(raw);
        assert_eq!(b.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), vec!["main", "dev", "origin/main"]);
        assert_eq!((b[0].ahead, b[0].behind), (1, 3));
        assert_eq!(b[2].remote_parts(), Some(("origin", "main")));
        assert_eq!(track("[gone]"), (0, 0, true));
    }

    #[test]
    fn remotes_fetch_y_push() {
        let r = remotes("origin\tgit@x:a.git (fetch)\norigin\tgit@y:a.git (push)\nup\thttps://u (fetch)\nup\thttps://u (push)\n");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].push_url, "git@y:a.git");
        assert_eq!(r[1].fetch_url, "https://u");
    }

    #[test]
    fn worktrees_porcelain() {
        let raw = "worktree /a\nHEAD 1234567890\nbranch refs/heads/main\n\nworktree /b\nHEAD abcdef0123\ndetached\nlocked razón\n\n";
        let w = worktrees(raw);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(w[0].head, "1234567");
        assert!(w[1].detached && w[1].locked);
    }

    #[test]
    fn name_status_con_renombre() {
        let f = name_status("M\0a.rs\0R087\0viejo\0nuevo\0A\0b.rs\0");
        assert_eq!(f.len(), 3);
        assert_eq!(f[1].status, 'R');
        assert_eq!(f[1].orig_path.as_deref(), Some("viejo"));
        assert_eq!(f[1].path, "nuevo");
    }

    #[test]
    fn actividad_por_dia() {
        let now = 10 * 86_400;
        let raw = format!("{}\x1fAna\n{}\x1fAna\n{}\x1fLuis\n", now - 10, now - 86_400 - 5, 0);
        let (b, a) = activity(&raw, now, 5);
        assert_eq!(b, vec![0, 0, 0, 1, 1]);
        assert_eq!(a[0], ("Ana".to_string(), 2));
    }

    #[test]
    fn stash_tag_reflog() {
        assert_eq!(stashes("stash@{0}\x1f5\x1fWIP on main: x\n")[0].message, "WIP on main: x");
        assert!(tags("v1\x1fabc\x1ftag\x1f5\x1fRelease\n")[0].annotated);
        assert_eq!(reflog("abc\x1fHEAD@{0}\x1fcommit: x\x1f7\n")[0].selector, "HEAD@{0}");
    }
}
