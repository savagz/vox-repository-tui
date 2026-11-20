mod app;
mod config;
mod deps;
mod event;
mod git;
mod i18n;
mod logging;
mod setup;
mod theme;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use app::{App, PendingAction};
use color_eyre::Result;
use config::Config;
use crossterm::event::{Event, EventStream};
use event::AppEvent;
use futures_util::StreamExt;
use setup::{SystemCheckMode, SystemCheckOutcome, SystemCheckState};
use theme::{Status, Theme};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const USAGE: &str = "voxrepository-tui: administración de repositorios git

USO:
    voxrepository-tui [OPCIONES] [RUTA]

    Sin RUTA abre el repo del directorio actual (si lo hay) y escanea el
    workspace configurado. Con RUTA abre ese repo.

OPCIONES:
    -w, --workspace  Arranca en el workspace aunque el directorio sea un repo
    -V, --version    Muestra la versión y sale
    -h, --help       Muestra esta ayuda y sale";

/// Hay un comando interactivo usando la terminal.
static INTERACTIVE: AtomicBool = AtomicBool::new(false);

enum Cli {
    Exit,
    Run { open: Option<PathBuf> },
}

fn handle_cli_args() -> Cli {
    let mut workspace = false;
    let mut path: Option<PathBuf> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-V" | "--version" => {
                println!("voxrepository-tui {}", env!("CARGO_PKG_VERSION"));
                return Cli::Exit;
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Cli::Exit;
            }
            "-w" | "--workspace" => workspace = true,
            other if other.starts_with('-') => {
                eprintln!("opción desconocida: {other}\n\n{USAGE}");
                return Cli::Exit;
            }
            other => path = Some(config::expand_home(other)),
        }
    }
    if let Some(p) = path {
        if !p.is_dir() {
            eprintln!("no es un directorio: {}", p.display());
            return Cli::Exit;
        }
        return Cli::Run { open: Some(p) };
    }
    if workspace {
        return Cli::Run { open: None };
    }
    let cwd = std::env::current_dir().ok();
    Cli::Run { open: cwd.filter(|d| inside_repo(d)) }
}

/// Busca `.git` subiendo desde `dir`, sin lanzar git.
fn inside_repo(dir: &Path) -> bool {
    dir.ancestors().any(|d| d.join(".git").exists())
}

#[tokio::main]
async fn main() -> Result<()> {
    let open = match handle_cli_args() {
        Cli::Exit => return Ok(()),
        Cli::Run { open } => open,
    };
    color_eyre::install()?;
    let _log_guard = logging::init()?;

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, open).await;
    ratatui::restore();
    result
}

async fn run(terminal: &mut ratatui::DefaultTerminal, open: Option<PathBuf>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);
    let mut input = Input::new(tx.clone());
    input.start();
    spawn_signal_task(tx.clone());
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "inicio de sesión");

    let (config, notice) = match Config::load() {
        Ok(Some(c)) => (c, None),
        Ok(None) => (Config::default(), None),
        Err(e) => {
            let message = format!("{e:#}");
            tracing::error!(error = %message, "config ilegible");
            (Config::default(), Some(message))
        }
    };
    let probe = deps::Probe { roots: config.roots(), config_path: Config::existing_path() };
    let mut system_check = SystemCheckState::new(config.ui.language, probe);

    // Sin git no hay nada que mostrar: el wizard se queda hasta que aparezca
    // o el usuario decida entrar igual.
    if !system_check.report.all_blocking_ok() {
        let theme = Theme::default();
        loop {
            terminal.draw(|f| ui::setup::render_gate(f, f.area(), &system_check, &theme))?;
            let Some(event) = rx.recv().await else { return Ok(()) };
            match event {
                AppEvent::Key(key) => match system_check.on_key(key, SystemCheckMode::Gate) {
                    SystemCheckOutcome::Recheck => {
                        system_check.recheck();
                        if system_check.report.all_blocking_ok() {
                            break;
                        }
                    }
                    SystemCheckOutcome::Continue => break,
                    SystemCheckOutcome::Cancel => return Ok(()),
                    SystemCheckOutcome::None => {}
                },
                AppEvent::Shutdown => return Ok(()),
                _ => {}
            }
        }
    }

    let tick_rate = Duration::from_millis(config.ui.tick_rate_ms.max(50));
    spawn_tick_task(tx.clone(), tick_rate);

    let mut app = App::new(config, tx.clone(), system_check);
    if let Some(msg) = notice {
        app.toast = Some((msg, Status::Error));
    }
    app.boot(open);
    terminal.draw(|f| ui::draw(f, &app))?;

    while let Some(event) = rx.recv().await {
        // Varios eventos juntos se procesan antes de redibujar una sola vez.
        app.on_event(event);
        while let Ok(more) = rx.try_recv() {
            app.on_event(more);
            if app.should_quit {
                break;
            }
        }
        if let Some(PendingAction::Interactive { script, args, cwd, pause }) = app.take_pending() {
            let result = run_interactive(terminal, &mut input, &script, &args, &cwd, pause, &app);
            app.after_interactive(result);
        }
        if app.should_quit {
            break;
        }
        terminal.draw(|f| ui::draw(f, &app))?;
    }
    input.stop();
    tracing::info!("fin de sesión");
    Ok(())
}

/// Suspende la TUI, corre `sh -c script` con la terminal real y la restaura.
fn run_interactive(
    terminal: &mut ratatui::DefaultTerminal,
    input: &mut Input,
    script: &str,
    args: &[String],
    cwd: &Path,
    pause: bool,
    app: &App,
) -> std::result::Result<(), String> {
    input.stop();
    ratatui::restore();
    INTERACTIVE.store(true, Ordering::SeqCst);
    let status = std::process::Command::new("sh").arg("-c").arg(script).arg("sh").args(args).current_dir(cwd).status();
    if pause {
        println!("\n{}", app.t(i18n::Key::PressEnter));
        let mut discard = String::new();
        let _ = std::io::stdin().read_line(&mut discard);
    }
    INTERACTIVE.store(false, Ordering::SeqCst);
    *terminal = ratatui::init();
    input.start();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(i18n::fill(app.t(i18n::Key::InteractiveExitTpl), &[&s.code().unwrap_or(-1).to_string()])),
        Err(e) => Err(e.to_string()),
    }
}

struct Input {
    tx: mpsc::Sender<AppEvent>,
    handle: Option<JoinHandle<()>>,
}

impl Input {
    fn new(tx: mpsc::Sender<AppEvent>) -> Self {
        Self { tx, handle: None }
    }

    fn start(&mut self) {
        if self.handle.is_none() {
            let tx = self.tx.clone();
            self.handle = Some(tokio::spawn(async move {
                let mut stream = EventStream::new();
                while let Some(Ok(evt)) = stream.next().await {
                    let ev = match evt {
                        Event::Key(key) => AppEvent::Key(key),
                        Event::Resize(..) => AppEvent::Redraw,
                        _ => continue,
                    };
                    if tx.send(ev).await.is_err() {
                        break;
                    }
                }
            }));
        }
    }

    fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

fn spawn_tick_task(tx: mpsc::Sender<AppEvent>, tick_rate: Duration) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tick_rate);
        loop {
            interval.tick().await;
            if tx.send(AppEvent::Tick).await.is_err() {
                break;
            }
        }
    });
}

/// SIGTERM, SIGHUP y SIGINT por `kill` no llegan como teclas: sin esto el
/// proceso muere sin restaurar la terminal.
#[cfg(unix)]
fn spawn_signal_task(tx: mpsc::Sender<AppEvent>) {
    use tokio::signal::unix::{SignalKind, signal};
    tokio::spawn(async move {
        let (Ok(mut term), Ok(mut hup), Ok(mut int)) = (signal(SignalKind::terminate()), signal(SignalKind::hangup()), signal(SignalKind::interrupt())) else {
            return;
        };
        loop {
            tokio::select! {
                _ = term.recv() => break,
                _ = hup.recv() => break,
                // Un ^C dentro del editor o la shell también llega acá (mismo
                // grupo de procesos): no es para la TUI.
                _ = int.recv() => if !INTERACTIVE.load(Ordering::SeqCst) { break },
            }
        }
        let _ = tx.send(AppEvent::Shutdown).await;
    });
}

#[cfg(not(unix))]
fn spawn_signal_task(tx: mpsc::Sender<AppEvent>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = tx.send(AppEvent::Shutdown).await;
        }
    });
}
