//! Planificador de rebase interactivo: la lista de tareas se edita en la TUI
//! (acción por commit, reordenar) y se ejecuta de una vez.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::git::rebase::{self, Action, Item, PlanError};

#[derive(Debug, Clone)]
pub struct Planner {
    /// Commit base (exclusivo); `None` = desde la raíz.
    pub base: Option<String>,
    /// Orden de aplicación: el más viejo primero.
    pub items: Vec<Item>,
    pub sel: usize,
    pub error: Option<PlanError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannerOutcome {
    None,
    Cancel,
    Execute,
    /// Pedir el mensaje nuevo del ítem.
    Reword(usize),
}

impl Planner {
    pub fn new(base: Option<String>, items: Vec<Item>) -> Self {
        let sel = items.len().saturating_sub(1);
        Self { base, items, sel, error: None }
    }

    fn set(&mut self, action: Action) {
        if let Some(item) = self.items.get_mut(self.sel) {
            item.action = action;
            if action != Action::Reword {
                item.message = None;
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> PlannerOutcome {
        self.error = None;
        let n = self.items.len();
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return PlannerOutcome::Cancel,
            KeyCode::Up if shift => self.move_by(-1),
            KeyCode::Down if shift => self.move_by(1),
            KeyCode::Char('K') => self.move_by(-1),
            KeyCode::Char('J') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            KeyCode::Char('p') => self.set(Action::Pick),
            KeyCode::Char('r') => {
                self.set(Action::Reword);
                return PlannerOutcome::Reword(self.sel);
            }
            KeyCode::Char('e') => self.set(Action::Edit),
            KeyCode::Char('s') => self.set(Action::Squash),
            KeyCode::Char('f') => self.set(Action::Fixup),
            KeyCode::Char('d') => self.set(Action::Drop),
            KeyCode::Enter => match rebase::validate(&self.items) {
                Ok(()) => return PlannerOutcome::Execute,
                Err(e) => self.error = Some(e),
            },
            _ => {}
        }
        PlannerOutcome::None
    }

    fn move_by(&mut self, delta: isize) {
        let target = self.sel as isize + delta;
        if target < 0 || target as usize >= self.items.len() {
            return;
        }
        self.items.swap(self.sel, target as usize);
        self.sel = target as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn reordena_y_valida() {
        let mut p = Planner::new(None, vec![Item::pick("a", "1"), Item::pick("b", "2")]);
        assert_eq!(p.sel, 1);
        p.on_key(k(KeyCode::Char('K')));
        assert_eq!(p.items[0].hash, "b");
        assert_eq!(p.sel, 0);
        p.on_key(k(KeyCode::Char('s')));
        assert_eq!(p.on_key(k(KeyCode::Enter)), PlannerOutcome::None);
        assert_eq!(p.error, Some(PlanError::MeldFirst));
        p.on_key(k(KeyCode::Char('p')));
        assert_eq!(p.on_key(k(KeyCode::Enter)), PlannerOutcome::Execute);
    }
}
