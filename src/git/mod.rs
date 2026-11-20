pub mod conflict;
#[cfg(test)]
mod integration_tests;
pub mod load;
pub mod model;
pub mod ops;
pub mod parse;
pub mod patch;
pub mod rebase;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Los errores de git se muestran tal cual al usuario (stderr recortado): un
/// tipo propio no aportaría nada que un `String` no tenga.
pub type GitResult<T> = Result<T, String>;

/// Ejecutor del CLI `git` sobre un directorio de trabajo. Todo lo que hace la
/// TUI pasa por acá, así el entorno (sin prompts, sin colores, sin terminal
/// controladora) es el mismo para cada comando.
#[derive(Debug, Clone)]
pub struct Git {
    pub workdir: PathBuf,
    pub timeout: Duration,
    pub net_timeout: Duration,
}

#[derive(Debug, Default, Clone)]
pub struct ExecOpts {
    pub stdin: Option<String>,
    /// Comando de red (fetch/pull/push): usa el timeout largo.
    pub net: bool,
    pub env: Vec<(String, String)>,
    /// Códigos de salida que no son error (p. ej. `diff --no-index` sale con 1
    /// cuando hay diferencias).
    pub ok_codes: Vec<i32>,
}

impl ExecOpts {
    pub fn net() -> Self {
        Self { net: true, ..Self::default() }
    }

    pub fn stdin(input: String) -> Self {
        Self { stdin: Some(input), ..Self::default() }
    }
}

impl Git {
    pub fn new(workdir: impl Into<PathBuf>, timeout_secs: u64, net_timeout_secs: u64) -> Self {
        Self {
            workdir: workdir.into(),
            timeout: Duration::from_secs(timeout_secs.max(1)),
            net_timeout: Duration::from_secs(net_timeout_secs.max(1)),
        }
    }

    pub fn at(&self, workdir: &Path) -> Self {
        Self { workdir: workdir.to_path_buf(), ..self.clone() }
    }

    pub async fn run<S: AsRef<str>>(&self, args: &[S]) -> GitResult<String> {
        self.exec(args, ExecOpts::default()).await
    }

    pub async fn run_net<S: AsRef<str>>(&self, args: &[S]) -> GitResult<String> {
        self.exec(args, ExecOpts::net()).await
    }

    pub async fn exec<S: AsRef<str>>(&self, args: &[S], opts: ExecOpts) -> GitResult<String> {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.workdir)
            .args(["-c", "color.ui=never", "-c", "core.quotepath=false", "-c", "advice.detachedHead=false"])
            .args(args.iter().map(|a| a.as_ref()))
            // Sin prompts de credenciales (fallan en vez de colgar la TUI),
            // sin locks opcionales (el polling no pelea con el usuario por
            // index.lock), salida en inglés estable y nunca un editor.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("LC_ALL", "C")
            .env("GIT_EDITOR", "true")
            .env("GIT_PAGER", "cat")
            .stdin(if opts.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        detach_from_terminal(&mut cmd);

        let timeout = if opts.net { self.net_timeout } else { self.timeout };
        let label = args.iter().map(|a| a.as_ref()).collect::<Vec<_>>().join(" ");

        let mut child = cmd.spawn().map_err(|e| format!("git {label}: {e}"))?;
        if let Some(input) = opts.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            tokio::spawn(async move {
                let _ = stdin.write_all(input.as_bytes()).await;
            });
        }

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(format!("git {label}: {e}")),
            Err(_) => return Err(format!("git {label}: timeout ({}s)", timeout.as_secs())),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        if output.status.success() || opts.ok_codes.contains(&code) {
            // push/fetch/pull escriben su resumen en stderr: sin stdout, eso
            // es lo que vale la pena mostrar en el registro de comandos.
            if opts.net && stdout.trim().is_empty() {
                return Ok(stderr.into_owned());
            }
            return Ok(stdout);
        }
        let msg = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("git {label}: exit {code}")
        };
        Err(msg)
    }
}

/// Corre git en su propia sesión, sin terminal controladora: así ssh o un
/// helper de credenciales no pueden abrir `/dev/tty` (la TUI está en modo raw)
/// y fallan enseguida en vez de esperar una contraseña invisible.
#[cfg(unix)]
fn detach_from_terminal(cmd: &mut Command) {
    // SAFETY: setsid es async-signal-safe y no toca memoria del proceso padre.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn detach_from_terminal(_cmd: &mut Command) {}

/// `git --version` sin repo: lo usa la verificación del sistema.
pub fn version_blocking() -> Option<String> {
    let out = std::process::Command::new("git").arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim().strip_prefix("git version ").map(|v| v.trim().to_string())
}

/// `git config --global --get <key>` sin repo.
pub fn global_config_blocking(key: &str) -> Option<String> {
    let out = std::process::Command::new("git").args(["config", "--get", key]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Comillas simples para `sh`: lo usan el editor de secuencia del rebase y
/// los scripts que corren con la terminal suspendida.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Parte una línea como lo haría un shell simple: espacios separan, comillas
/// simples y dobles agrupan, `\` escapa. Para el comando libre `:`.
pub fn split_args(line: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                in_arg = true;
            }
            (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    in_arg = true;
                }
            }
            (None, c) if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut cur));
                    in_arg = false;
                }
            }
            (None, c) => {
                cur.push(c);
                in_arg = true;
            }
        }
    }
    if quote.is_some() {
        return Err("unterminated quote".into());
    }
    if in_arg {
        args.push(cur);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_args_respeta_comillas() {
        assert_eq!(split_args(r#"log --format="%h %s" -n 3"#).unwrap(), vec!["log", "--format=%h %s", "-n", "3"]);
        assert_eq!(split_args("commit -m 'it''s'").unwrap(), vec!["commit", "-m", "its"]);
        assert_eq!(split_args(r"add a\ b").unwrap(), vec!["add", "a b"]);
        assert!(split_args("x 'y").is_err());
        assert_eq!(split_args("  ").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn sh_quote_escapa_comilla_simple() {
        assert_eq!(sh_quote("a'b"), "'a'\\''b'");
    }
}
