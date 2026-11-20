//! Resolución de conflictos bloque por bloque dentro de STATUS.

use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use crate::git::conflict::{self, Choice, Segment};
use crate::git::ops::{self, Op};
use crate::i18n::{self, Key};
use crate::theme::Status;

#[derive(Debug, Clone)]
pub struct ConflictView {
    pub path: String,
    pub segments: Vec<Segment>,
    pub sel: usize,
}

impl ConflictView {
    pub fn count(&self) -> usize {
        conflict::blocks(&self.segments).count()
    }

    pub fn unresolved(&self) -> usize {
        conflict::blocks(&self.segments).filter(|b| b.choice == Choice::Unresolved).count()
    }
}

impl App {
    pub(crate) fn open_conflict(&mut self) {
        let Some(repo) = &mut self.repo else { return };
        let Some(entry) = repo.selected_entry().cloned() else { return };
        let full = repo.ctx.root.join(&entry.path);
        match std::fs::read_to_string(&full) {
            Ok(text) => {
                let view = ConflictView { path: entry.path.clone(), segments: conflict::parse(&text), sel: 0 };
                if view.count() == 0 {
                    self.notify(self.t(Key::NoConflictMarkers), Status::Info);
                    return;
                }
                repo.conflict = Some(view);
            }
            Err(e) => self.notify(format!("{}: {e}", full.display()), Status::Error),
        }
    }

    /// Teclas con el resolvedor abierto. Siempre consume la tecla salvo las
    /// globales de salida.
    pub(crate) fn conflict_key(&mut self, key: KeyEvent) -> bool {
        let Some(repo) = &mut self.repo else { return false };
        let Some(view) = &mut repo.conflict else { return false };
        let n = view.count();
        let set = |view: &mut ConflictView, c: Choice| {
            if let Some(b) = conflict::block_mut(&mut view.segments, view.sel) {
                b.choice = c;
            }
            if view.sel + 1 < view.count() {
                view.sel += 1;
            }
        };
        match key.code {
            KeyCode::Esc => repo.conflict = None,
            KeyCode::Up | KeyCode::Char('k') => view.sel = view.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => view.sel = (view.sel + 1).min(n.saturating_sub(1)),
            KeyCode::Char('o') => set(view, Choice::Ours),
            KeyCode::Char('t') => set(view, Choice::Theirs),
            KeyCode::Char('b') => set(view, Choice::Both),
            KeyCode::Char('u') => set(view, Choice::Unresolved),
            KeyCode::Char('e') => {
                let path = view.path.clone();
                repo.conflict = None;
                self.open_editor(&path);
            }
            KeyCode::Enter | KeyCode::Char('w') => {
                let contents = conflict::render(&view.segments);
                let path = view.path.clone();
                let resolved = view.unresolved() == 0;
                repo.conflict = None;
                let mut steps = vec![Op::WriteFile { path: path.clone().into(), contents }];
                if resolved {
                    steps.push(ops::git(["add", "--", path.as_str()]));
                }
                let desc = i18n::fill(self.t(if resolved { Key::DescResolved } else { Key::DescConflictSaved }), &[&path]);
                self.run_op(desc, Op::Seq(steps), super::Follow::None);
            }
            KeyCode::Char('q') => return false,
            _ => {}
        }
        true
    }
}
