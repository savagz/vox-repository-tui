use crossterm::event::{KeyCode, KeyEvent};

use crate::deps::{self, DepReport, Probe};
use crate::i18n::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemCheckMode {
    /// Wizard a pantalla completa antes del tablero.
    Gate,
    /// Embebido como pestaña.
    Panel,
}

#[derive(Debug, Clone)]
pub enum SystemCheckOutcome {
    None,
    Recheck,
    Continue,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct SystemCheckState {
    pub report: DepReport,
    pub selected: usize,
    pub lang: Lang,
    probe: Probe,
}

impl SystemCheckState {
    pub fn new(lang: Lang, probe: Probe) -> Self {
        Self { report: deps::probe(lang, &probe), selected: 0, lang, probe }
    }

    pub fn recheck(&mut self) {
        self.report = deps::probe(self.lang, &self.probe);
        self.selected = self.selected.min(self.report.checks.len().saturating_sub(1));
    }

    pub fn set_probe(&mut self, probe: Probe) {
        self.probe = probe;
        self.recheck();
    }

    pub fn set_lang(&mut self, lang: Lang) {
        if self.lang != lang {
            self.lang = lang;
            self.recheck();
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, mode: SystemCheckMode) -> SystemCheckOutcome {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                SystemCheckOutcome::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.report.checks.len().saturating_sub(1));
                SystemCheckOutcome::None
            }
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::F(5) => SystemCheckOutcome::Recheck,
            KeyCode::Char('a') if mode == SystemCheckMode::Gate => SystemCheckOutcome::Continue,
            KeyCode::Esc | KeyCode::Char('q') if mode == SystemCheckMode::Gate => SystemCheckOutcome::Cancel,
            _ => SystemCheckOutcome::None,
        }
    }
}
