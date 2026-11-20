pub mod conflicts;
pub mod intent;
pub mod keys;
pub mod modal;
pub mod planner;
pub mod repo;
pub mod staging;
pub mod workspace;

use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc;

use crate::config::{Config, ConfigForm, ConfigOutcome};
use crate::event::AppEvent;
use crate::git::ops::{self, Op};
use crate::i18n::{self, Key, Lang};
use crate::setup::{SystemCheckMode, SystemCheckOutcome, SystemCheckState};
use crate::theme::{Status, Theme};
use crate::ui::tabs::Tab;

use intent::Intent;
use modal::{Modal, Outcome};
use planner::PlannerOutcome;
pub use repo::RepoState;
use workspace::WsState;

const LOG_CAPACITY: usize = 200;

/// Qué hacer cuando termina una operación, además de refrescar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Follow {
    None,
    /// Si el push fue rechazado, ofrecer `--force-with-lease`.
    OfferForcePush,
    /// Mostrar la salida completa en un modal con este título.
    ShowOutput(String),
}

/// Acciones que necesitan la terminal: el bucle principal la suspende, corre
/// el comando y la restaura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingAction {
    Interactive {
        /// Script de `sh -c`; recibe `args` como `$1..`.
        script: String,
        args: Vec<String>,
        cwd: PathBuf,
        /// Esperar ENTER antes de volver (para leer la salida).
        pause: bool,
    },
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub time: DateTime<Local>,
    pub desc: String,
    pub ok: bool,
    pub output: String,
}

pub struct App {
    pub config: Config,
    pub lang: Lang,
    pub theme: Theme,
    tx: mpsc::Sender<AppEvent>,
    pub active_tab: Tab,
    pub should_quit: bool,
    pub toast: Option<(String, Status)>,
    pub help_open: bool,
    pub help_scroll: u16,
    pub modal: Option<Modal>,
    pending: Option<PendingAction>,
    pub system_check: SystemCheckState,
    pub cmd_log: VecDeque<LogEntry>,
    /// Operaciones de escritura en curso sobre el repo abierto.
    pub busy: usize,
    pub busy_desc: String,
    pub tick: u64,
    pub ws: WsState,
    pub repo: Option<RepoState>,
    pub opening: bool,
    next_gen: u64,
}

impl App {
    pub fn new(config: Config, tx: mpsc::Sender<AppEvent>, system_check: SystemCheckState) -> Self {
        let lang = config.ui.language;
        Self {
            ws: WsState::new(),
            config,
            lang,
            theme: Theme::default(),
            tx,
            active_tab: Tab::Workspace,
            should_quit: false,
            toast: None,
            help_open: false,
            help_scroll: 0,
            modal: None,
            pending: None,
            system_check,
            cmd_log: VecDeque::new(),
            busy: 0,
            busy_desc: String::new(),
            tick: 0,
            repo: None,
            opening: false,
            next_gen: 0,
        }
    }

    /// Arranque: escanea el workspace y, si se pidió, abre un repo.
    pub fn boot(&mut self, open: Option<PathBuf>) {
        self.rescan_workspace();
        if let Some(path) = open {
            self.open_repo(path);
        }
    }

    pub fn t(&self, key: Key) -> &'static str {
        i18n::t(self.lang, key)
    }

    pub fn take_pending(&mut self) -> Option<PendingAction> {
        self.pending.take()
    }

    pub(crate) fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = AppEvent> + Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(fut.await).await;
        });
    }

    fn next_generation(&mut self) -> u64 {
        self.next_gen += 1;
        self.next_gen
    }

    pub(crate) fn notify(&mut self, msg: impl Into<String>, status: Status) {
        self.toast = Some((msg.into(), status));
    }

    pub(crate) fn push_log(&mut self, desc: &str, ok: bool, output: &str) {
        if self.cmd_log.len() >= LOG_CAPACITY {
            self.cmd_log.pop_front();
        }
        self.cmd_log.push_back(LogEntry { time: Local::now(), desc: desc.to_string(), ok, output: output.trim_end().to_string() });
    }

    /// Lanza una operación de escritura sobre el repo abierto.
    pub(crate) fn run_op(&mut self, desc: String, op: Op, follow: Follow) {
        let Some(repo) = &self.repo else {
            self.notify(self.t(Key::NoRepo), Status::Warning);
            return;
        };
        if self.busy > 0 {
            self.notify(i18n::fill(self.t(Key::BusyTpl), &[&self.busy_desc]), Status::Warning);
            return;
        }
        self.busy += 1;
        self.busy_desc = desc.clone();
        let git = repo.git.clone();
        let ctx = repo.ctx.clone();
        let generation = repo.generation;
        self.spawn(async move {
            let result = ops::execute(&git, &ctx, op).await;
            AppEvent::OpDone { generation, desc, result, follow }
        });
    }

    pub fn on_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Tick => self.on_tick(),
            AppEvent::Key(key) => self.on_key(key),
            AppEvent::Redraw => {}
            AppEvent::Shutdown => self.should_quit = true,
            AppEvent::WsFound { scan, paths } => self.ws_found(scan, paths),
            AppEvent::WsSummary { scan, path, result } => self.ws_summary(scan, path, result),
            AppEvent::WsOpDone { path, desc, result } => self.ws_op_done(path, desc, result),
            AppEvent::RepoOpened { result } => self.repo_opened(result),
            AppEvent::Snapshot { generation, snap } => self.apply_snapshot(generation, *snap),
            AppEvent::StatusPoll { generation, status, op } => self.apply_status(generation, status, op),
            AppEvent::Detail { generation, req, content } => self.apply_detail(generation, req, content),
            AppEvent::OpDone { generation, desc, result, follow } => self.op_done(generation, desc, result, follow),
        }
    }

    fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.poll_repo();
        self.poll_workspace();
    }

    fn op_done(&mut self, generation: u64, desc: String, result: Result<String, String>, follow: Follow) {
        self.busy = self.busy.saturating_sub(1);
        match &result {
            Ok(out) => {
                self.push_log(&desc, true, out);
                self.notify(i18n::fill(self.t(Key::OpOkTpl), &[&desc]), Status::Ok);
            }
            Err(e) => {
                self.push_log(&desc, false, e);
                let first = e.lines().find(|l| !l.trim().is_empty()).unwrap_or(e).trim();
                self.notify(i18n::fill(self.t(Key::OpErrTpl), &[&desc, first]), Status::Error);
            }
        }
        match follow {
            Follow::None => {}
            Follow::OfferForcePush => {
                if let Err(e) = &result
                    && (e.contains("rejected") || e.contains("non-fast-forward") || e.contains("fetch first"))
                {
                    self.start(Intent::ForcePush);
                }
            }
            Follow::ShowOutput(title) => {
                let (text, err) = match &result {
                    Ok(t) => (t.as_str(), false),
                    Err(e) => (e.as_str(), true),
                };
                self.modal = Some(Modal::Output(modal::Output::new(title, text, err)));
            }
        }
        let path = self.repo.as_ref().filter(|r| r.generation == generation).map(|r| r.ctx.root.clone());
        if let Some(path) = path {
            self.refresh_repo(true);
            self.ws_refresh_one(&path);
        }
    }

    /// Vuelta de un comando interactivo (editor, shell, commit con editor).
    pub fn after_interactive(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => self.notify(self.t(Key::InteractiveDone), Status::Info),
            Err(e) => self.notify(e, Status::Error),
        }
        self.refresh_repo(true);
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        self.toast = None;

        if self.help_open {
            self.on_help_key(key);
            return;
        }
        if let Some(modal) = self.modal.take() {
            self.on_modal_key(modal, key);
            return;
        }
        if self.on_filter_key(key) {
            return;
        }
        if self.active_tab == Tab::SystemCheck {
            match self.system_check.on_key(key, SystemCheckMode::Panel) {
                SystemCheckOutcome::Recheck => {
                    self.system_check.recheck();
                    return;
                }
                SystemCheckOutcome::None if matches!(key.code, KeyCode::Up | KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('k')) => return,
                _ => {}
            }
        }
        if self.tab_key(key) {
            return;
        }
        self.global_key(key);
    }

    fn on_help_key(&mut self, key: KeyEvent) {
        let max = crate::ui::widgets::help::line_count(self.lang).saturating_sub(1);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.help_scroll = self.help_scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.help_scroll = (self.help_scroll + 1).min(max),
            KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
            KeyCode::PageDown => self.help_scroll = (self.help_scroll + 10).min(max),
            KeyCode::Home | KeyCode::Char('g') => self.help_scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.help_scroll = max,
            _ => self.help_open = false,
        }
    }

    fn on_modal_key(&mut self, modal: Modal, key: KeyEvent) {
        match modal {
            Modal::Form(mut form) => match form.on_key(key) {
                Outcome::Submit(values) => self.run_intent(form.intent, values),
                Outcome::Cancel => self.cancel_intent(form.intent),
                Outcome::Invalid => {
                    form.error = Some(self.t(Key::FieldRequired).to_string());
                    self.modal = Some(Modal::Form(form));
                }
                _ => self.modal = Some(Modal::Form(form)),
            },
            Modal::Confirm(mut c) => match c.on_key(key) {
                Outcome::Submit(_) => self.run_intent(c.intent, Vec::new()),
                Outcome::Cancel => self.cancel_intent(c.intent),
                _ => self.modal = Some(Modal::Confirm(c)),
            },
            Modal::Menu(mut m) => match m.on_key(key) {
                Outcome::Pick(i) => {
                    if let Some(item) = m.items.get(i) {
                        self.start(item.intent.clone());
                    }
                }
                Outcome::Cancel => {}
                _ => self.modal = Some(Modal::Menu(m)),
            },
            Modal::Output(mut o) => {
                if o.on_key(key) != Outcome::Cancel {
                    self.modal = Some(Modal::Output(o));
                }
            }
            Modal::Commit(mut c) => match c.on_key(key) {
                Outcome::Submit(values) => self.run_intent(c.intent, values),
                Outcome::Cancel => {}
                Outcome::Invalid => {
                    c.error = Some(self.t(Key::CommitEmpty).to_string());
                    self.modal = Some(Modal::Commit(c));
                }
                _ => self.modal = Some(Modal::Commit(c)),
            },
            Modal::Planner(mut p) => match p.on_key(key) {
                PlannerOutcome::Execute => {
                    let (base, items) = (p.base.clone(), p.items.clone());
                    self.run_intent(Intent::ExecuteRebase { base, items }, Vec::new());
                }
                PlannerOutcome::Cancel => {}
                PlannerOutcome::Reword(idx) => {
                    let current = p.items[idx].message.clone().unwrap_or_else(|| p.items[idx].subject.clone());
                    let form = modal::Form::new(
                        self.t(Key::FormReword),
                        vec![modal::Field::text(self.t(Key::FieldMessage), current)],
                        Intent::PlannerReword { planner: p, idx },
                    );
                    self.modal = Some(Modal::Form(form));
                }
                PlannerOutcome::None => self.modal = Some(Modal::Planner(p)),
            },
            Modal::Config(mut form) => match form.on_key(key) {
                ConfigOutcome::Save(config) => self.apply_config(*config),
                ConfigOutcome::Cancel => {}
                ConfigOutcome::None => self.modal = Some(Modal::Config(form)),
            },
        }
    }

    fn apply_config(&mut self, config: Config) {
        let roots_changed = config.workspace != self.config.workspace;
        self.lang = config.ui.language;
        self.system_check.set_lang(self.lang);
        self.config = config;
        match self.config.save() {
            Ok(path) => self.notify(i18n::fill(self.t(Key::ConfigSavedTpl), &[&path.display().to_string()]), Status::Ok),
            Err(e) => self.notify(format!("{e:#}"), Status::Error),
        }
        self.system_check.set_probe(crate::deps::Probe { roots: self.config.roots(), config_path: Config::existing_path() });
        if let Some(repo) = &mut self.repo {
            repo.git = crate::git::Git::new(&repo.ctx.root, self.config.git.command_timeout_secs, self.config.git.network_timeout_secs);
            repo.log_opts.limit = self.config.git.log_limit;
        }
        if roots_changed {
            self.rescan_workspace();
        }
    }

    fn global_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let in_repo = self.repo.is_some() && self.active_tab.needs_repo();
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => self.switch_tab(self.active_tab.next(), 1),
            KeyCode::BackTab => self.switch_tab(self.active_tab.prev(), -1),
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(tab) = Tab::from_digit(c.to_digit(10).unwrap_or(0)) {
                    self.goto_tab(tab);
                }
            }
            KeyCode::Char('w') | KeyCode::Char('W') => self.goto_tab(Tab::Workspace),
            KeyCode::Char('h') | KeyCode::Char('?') => {
                self.help_open = true;
                self.help_scroll = 0;
            }
            KeyCode::F(2) => self.modal = Some(Modal::Config(Box::new(ConfigForm::new(&self.config)))),
            KeyCode::F(5) => self.refresh_current(),
            KeyCode::Char('r') if ctrl => self.refresh_current(),
            KeyCode::Char('L') => self.open_command_log(),
            KeyCode::Char(':') if self.repo.is_some() => self.start(Intent::GitCommand),
            KeyCode::Char('!') => self.open_shell(),
            KeyCode::Char('/') => self.begin_filter(),
            KeyCode::Char('p') if in_repo => self.start(Intent::Pull),
            KeyCode::Char('P') if in_repo => self.start(Intent::Push),
            KeyCode::Char('f') if in_repo => self.start(Intent::FetchAll),
            KeyCode::Char('m') if in_repo => self.open_op_menu(),
            KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
            KeyCode::Home | KeyCode::Char('g') => self.move_sel(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.move_sel(isize::MAX / 2),
            KeyCode::PageDown => self.scroll_detail(15),
            KeyCode::PageUp => self.scroll_detail(-15),
            KeyCode::Char('d') if ctrl => self.scroll_detail(15),
            KeyCode::Char('u') if ctrl => self.scroll_detail(-15),
            KeyCode::Esc => self.back(),
            _ => {}
        }
    }

    /// Cambia de pestaña en `dir` saltando las que piden repo si no hay uno.
    fn switch_tab(&mut self, mut tab: Tab, dir: isize) {
        if self.repo.is_none() {
            while tab.needs_repo() {
                tab = if dir > 0 { tab.next() } else { tab.prev() };
            }
        }
        self.set_tab(tab);
    }

    pub(crate) fn goto_tab(&mut self, tab: Tab) {
        if tab.needs_repo() && self.repo.is_none() {
            self.notify(self.t(Key::NoRepo), Status::Warning);
            return;
        }
        self.set_tab(tab);
    }

    fn set_tab(&mut self, tab: Tab) {
        if self.active_tab != tab {
            if let Some(repo) = &mut self.repo {
                repo.filter_clear();
            }
            self.ws.filter_editing = false;
        }
        self.active_tab = tab;
        if tab == Tab::SystemCheck {
            self.system_check.recheck();
        }
        self.refresh_detail(false);
    }

    fn refresh_current(&mut self) {
        match self.active_tab {
            Tab::Workspace => self.rescan_workspace(),
            Tab::SystemCheck => self.system_check.recheck(),
            _ => self.refresh_repo(true),
        }
        self.notify(self.t(Key::Refreshing), Status::Info);
    }

    fn open_command_log(&mut self) {
        let mut text = String::new();
        for e in self.cmd_log.iter().rev() {
            text.push_str(&format!("{} {} {}\n", e.time.format("%H:%M:%S"), if e.ok { "✔" } else { "✘" }, e.desc));
            for l in e.output.lines().take(40) {
                text.push_str(&format!("    {l}\n"));
            }
        }
        if text.is_empty() {
            text = self.t(Key::LogEmpty).to_string();
        }
        self.modal = Some(Modal::Output(modal::Output::new(self.t(Key::LogTitle), &text, false)));
    }

    /// Shell en el repo abierto o en el repo elegido del workspace.
    fn open_shell(&mut self) {
        let dir = if self.active_tab == Tab::Workspace {
            self.ws.selected_path()
        } else {
            self.repo.as_ref().map(|r| r.ctx.root.clone())
        };
        let Some(cwd) = dir else {
            self.notify(self.t(Key::NoRepo), Status::Warning);
            return;
        };
        self.pending = Some(PendingAction::Interactive {
            script: "exec \"${SHELL:-sh}\"".into(),
            args: Vec::new(),
            cwd,
            pause: false,
        });
    }

    pub(crate) fn open_editor(&mut self, path: &str) {
        let Some(repo) = &self.repo else { return };
        let editor = crate::deps::editor().unwrap_or_else(|| "vi".into());
        self.pending = Some(PendingAction::Interactive {
            script: format!("{editor} \"$1\""),
            args: vec![path.to_string()],
            cwd: repo.ctx.root.clone(),
            pause: false,
        });
    }

    pub(crate) fn commit_with_editor(&mut self) {
        let Some(repo) = &self.repo else { return };
        self.pending = Some(PendingAction::Interactive {
            script: "git commit".into(),
            args: Vec::new(),
            cwd: repo.ctx.root.clone(),
            pause: true,
        });
    }

    fn open_op_menu(&mut self) {
        let Some(repo) = &self.repo else { return };
        if repo.snap.op == crate::git::model::OpState::None {
            self.notify(self.t(Key::NoOpInProgress), Status::Info);
            return;
        }
        self.start(Intent::OpMenu);
    }

    pub(crate) fn copy_to_clipboard(&mut self, text: &str) {
        use std::io::Write;
        let Some(tool) = crate::deps::clipboard_tool() else {
            self.notify(self.t(Key::NoClipboard), Status::Warning);
            return;
        };
        let child = std::process::Command::new(tool)
            .args(crate::deps::clipboard_args(tool))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let ok = match child {
            Ok(mut c) => {
                let wrote = c.stdin.take().map(|mut s| s.write_all(text.as_bytes()).is_ok()).unwrap_or(false);
                c.wait().map(|s| s.success()).unwrap_or(false) && wrote
            }
            Err(_) => false,
        };
        if ok {
            self.notify(i18n::fill(self.t(Key::CopiedTpl), &[text]), Status::Ok);
        } else {
            self.notify(self.t(Key::NoClipboard), Status::Warning);
        }
    }

    fn back(&mut self) {
        if self.active_tab == Tab::Workspace {
            self.ws.filter.value.clear();
            self.ws.clamp();
            return;
        }
        let Some(repo) = &mut self.repo else { return };
        if !repo.filter.value.is_empty() {
            repo.filter_clear();
        } else if self.active_tab == Tab::Commits && repo.commit_files.is_some() {
            repo.commit_files = None;
        } else if self.active_tab == Tab::Commits && (repo.log_opts.reference.is_some() || repo.log_opts.all) {
            repo.log_opts.reference = None;
            repo.log_opts.all = false;
            repo.sel[Tab::Commits.index()] = 0;
            self.refresh_repo(true);
            return;
        }
        self.refresh_detail(false);
    }

    fn begin_filter(&mut self) {
        match self.active_tab {
            Tab::Workspace => self.ws.filter_editing = true,
            Tab::Status | Tab::Commits | Tab::Branches | Tab::Remotes | Tab::Stash | Tab::Tags | Tab::Worktrees | Tab::Reflog => {
                if let Some(repo) = &mut self.repo {
                    repo.filter_editing = true;
                }
            }
            _ => {}
        }
    }

    /// Teclas mientras se escribe un filtro. ENTER fija, ESC borra.
    fn on_filter_key(&mut self, key: KeyEvent) -> bool {
        if self.active_tab == Tab::Workspace {
            if !self.ws.filter_editing {
                return false;
            }
            match key.code {
                KeyCode::Enter => self.ws.filter_editing = false,
                KeyCode::Esc => {
                    self.ws.filter_editing = false;
                    self.ws.filter.value.clear();
                    self.ws.filter.cursor = 0;
                }
                _ => {
                    self.ws.filter.on_key(key);
                }
            }
            self.ws.sel = 0;
            self.ws.clamp();
            return true;
        }
        let Some(repo) = &mut self.repo else { return false };
        if !repo.filter_editing {
            return false;
        }
        match key.code {
            KeyCode::Enter => repo.filter_editing = false,
            KeyCode::Esc => repo.filter_clear(),
            _ => {
                repo.filter.on_key(key);
            }
        }
        repo.sel[self.active_tab.index()] = 0;
        self.refresh_detail(false);
        true
    }

    fn move_sel(&mut self, delta: isize) {
        if self.active_tab == Tab::Workspace {
            self.ws.move_sel(delta);
            return;
        }
        let tab = self.active_tab;
        let Some(repo) = &mut self.repo else { return };
        let len = repo.visible_len(tab);
        let sel = &mut repo.sel[tab.index()];
        *sel = clamp_move(*sel, delta, len);
        self.refresh_detail(false);
    }

    fn scroll_detail(&mut self, delta: i32) {
        if let Some(repo) = &mut self.repo {
            let max = repo.detail.line_count().saturating_sub(1) as i32;
            repo.detail.scroll = (repo.detail.scroll as i32 + delta).clamp(0, max.max(0)) as u16;
        }
    }

    /// Tiempo desde un instante, para los polls.
    pub(crate) fn elapsed(since: Instant, secs: u64) -> bool {
        since.elapsed() >= Duration::from_secs(secs)
    }
}

pub(crate) fn clamp_move(sel: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (sel as isize).saturating_add(delta).clamp(0, len as isize - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_move_respeta_bordes() {
        assert_eq!(clamp_move(0, -1, 5), 0);
        assert_eq!(clamp_move(3, 10, 5), 4);
        assert_eq!(clamp_move(2, isize::MIN / 2, 5), 0);
        assert_eq!(clamp_move(2, 1, 0), 0);
    }
}
