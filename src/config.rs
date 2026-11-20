use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::i18n::{self, Key, Lang};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    /// Carpetas donde buscar repos. Vacío = el directorio actual.
    pub roots: Vec<String>,
    pub max_depth: usize,
    /// Nombres de carpeta que no se recorren.
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PollConfig {
    /// Refresco del estado del árbol del repo abierto.
    pub status_secs: u64,
    /// Refresco completo (ramas, tags, stash...) del repo abierto.
    pub snapshot_secs: u64,
    /// Refresco de la tabla del workspace.
    pub workspace_secs: u64,
    /// `git fetch --all` automático del repo abierto (0 = nunca).
    pub auto_fetch_mins: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GitConfig {
    pub command_timeout_secs: u64,
    pub network_timeout_secs: u64,
    pub log_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub tick_rate_ms: u64,
    pub language: Lang,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub workspace: WorkspaceConfig,
    pub poll: PollConfig,
    pub git: GitConfig,
    pub ui: UiConfig,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            max_depth: 3,
            ignore: ["node_modules", "target", "vendor", "dist", "build", "Library"].map(String::from).to_vec(),
        }
    }
}

impl Default for PollConfig {
    fn default() -> Self {
        Self { status_secs: 2, snapshot_secs: 15, workspace_secs: 60, auto_fetch_mins: 0 }
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        Self { command_timeout_secs: 20, network_timeout_secs: 120, log_limit: 400 }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self { tick_rate_ms: 250, language: Lang::default() }
    }
}

impl Config {
    /// `Ok(None)`: no hay archivo, se usan los valores por defecto.
    pub fn load() -> Result<Option<Self>> {
        let Some(path) = Self::existing_path() else {
            return Ok(None);
        };
        let raw = std::fs::read_to_string(&path).wrap_err_with(|| format!("no se pudo leer la config en {}", path.display()))?;
        let config: Config = toml::from_str(&raw).wrap_err_with(|| format!("config inválida en {}", path.display()))?;
        Ok(Some(config))
    }

    pub fn save(&self) -> Result<PathBuf> {
        let lang = self.ui.language;
        let path = Self::write_path(lang)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).wrap_err_with(|| i18n::fill(i18n::t(lang, Key::ConfigErrCreateDir), &[&parent.display().to_string()]))?;
        }
        let body = toml::to_string_pretty(self).wrap_err(i18n::t(lang, Key::ConfigErrSerialize))?;
        let contents = format!("# Generado por voxrepository-tui.\n\n{body}");
        std::fs::write(&path, contents).wrap_err_with(|| i18n::fill(i18n::t(lang, Key::ConfigErrWrite), &[&path.display().to_string()]))?;
        Ok(path)
    }

    pub fn existing_path() -> Option<PathBuf> {
        if let Some(dev) = dev_config_path().filter(|p| p.exists()) {
            return Some(dev);
        }
        let user = user_config_path()?;
        user.exists().then_some(user)
    }

    fn write_path(lang: Lang) -> Result<PathBuf> {
        if let Some(path) = Self::existing_path() {
            return Ok(path);
        }
        user_config_path().ok_or_else(|| eyre!(i18n::t(lang, Key::ConfigErrNoUserDir)))
    }

    /// Raíces con `~` expandido; sin raíces configuradas, el directorio actual.
    pub fn roots(&self) -> Vec<PathBuf> {
        let roots: Vec<PathBuf> = self.workspace.roots.iter().filter(|r| !r.trim().is_empty()).map(|r| expand_home(r.trim())).collect();
        if roots.is_empty() {
            return std::env::current_dir().into_iter().collect();
        }
        roots
    }
}

pub fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/").or_else(|| (path == "~").then_some(""))
        && let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

/// Inverso de `expand_home`, para mostrar rutas cortas.
pub fn shorten_home(path: &std::path::Path) -> String {
    if let Some(home) = directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// `config/voxrepository.toml` relativo al directorio actual, sólo en builds
/// de desarrollo.
fn dev_config_path() -> Option<PathBuf> {
    cfg!(debug_assertions).then(|| PathBuf::from("config/voxrepository.toml"))
}

fn user_config_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "voxrepository-tui").map(|dirs| dirs.config_dir().join("config.toml"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    Roots,
    MaxDepth,
    StatusSecs,
    AutoFetch,
    LogLimit,
    NetTimeout,
    Language,
}

impl ConfigField {
    pub const ALL: [ConfigField; 7] = [
        ConfigField::Roots,
        ConfigField::MaxDepth,
        ConfigField::StatusSecs,
        ConfigField::AutoFetch,
        ConfigField::LogLimit,
        ConfigField::NetTimeout,
        ConfigField::Language,
    ];

    pub fn is_choice(self) -> bool {
        self == ConfigField::Language
    }

    pub fn label(self, lang: Lang) -> &'static str {
        i18n::t(
            lang,
            match self {
                ConfigField::Roots => Key::CfgRoots,
                ConfigField::MaxDepth => Key::CfgMaxDepth,
                ConfigField::StatusSecs => Key::CfgStatusSecs,
                ConfigField::AutoFetch => Key::CfgAutoFetch,
                ConfigField::LogLimit => Key::CfgLogLimit,
                ConfigField::NetTimeout => Key::CfgNetTimeout,
                ConfigField::Language => Key::CfgLanguage,
            },
        )
    }

    pub fn hint(self, lang: Lang) -> &'static str {
        i18n::t(
            lang,
            match self {
                ConfigField::Roots => Key::CfgRootsHint,
                ConfigField::MaxDepth => Key::CfgMaxDepthHint,
                ConfigField::StatusSecs => Key::CfgStatusSecsHint,
                ConfigField::AutoFetch => Key::CfgAutoFetchHint,
                ConfigField::LogLimit => Key::CfgLogLimitHint,
                ConfigField::NetTimeout => Key::CfgNetTimeoutHint,
                ConfigField::Language => Key::CfgLanguageHint,
            },
        )
    }
}

#[derive(Debug, Clone)]
pub enum ConfigOutcome {
    None,
    Save(Box<Config>),
    Cancel,
}

/// Overlay de configuración (F2).
#[derive(Debug, Clone)]
pub struct ConfigForm {
    pub focus: usize,
    values: [String; 6],
    pub language: Lang,
    pub error: Option<String>,
    base: Config,
}

impl ConfigForm {
    pub fn new(config: &Config) -> Self {
        Self {
            focus: 0,
            values: [
                config.workspace.roots.join(", "),
                config.workspace.max_depth.to_string(),
                config.poll.status_secs.to_string(),
                config.poll.auto_fetch_mins.to_string(),
                config.git.log_limit.to_string(),
                config.git.network_timeout_secs.to_string(),
            ],
            language: config.ui.language,
            error: None,
            base: config.clone(),
        }
    }

    pub fn field(&self) -> ConfigField {
        ConfigField::ALL[self.focus % ConfigField::ALL.len()]
    }

    pub fn value_of(&self, field: ConfigField) -> String {
        match field {
            ConfigField::Language => self.language.label().to_string(),
            f => self.values[f as usize].clone(),
        }
    }

    pub fn on_key(&mut self, key: crossterm::event::KeyEvent) -> ConfigOutcome {
        use crossterm::event::KeyCode;
        self.error = None;
        let field = self.field();
        match key.code {
            KeyCode::Esc => ConfigOutcome::Cancel,
            KeyCode::Up | KeyCode::BackTab => {
                self.focus = (self.focus + ConfigField::ALL.len() - 1) % ConfigField::ALL.len();
                ConfigOutcome::None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.focus = (self.focus + 1) % ConfigField::ALL.len();
                ConfigOutcome::None
            }
            KeyCode::Left | KeyCode::Right if field.is_choice() => {
                self.language = self.language.toggle();
                ConfigOutcome::None
            }
            KeyCode::Backspace if !field.is_choice() => {
                self.values[field as usize].pop();
                ConfigOutcome::None
            }
            KeyCode::Char(c) if !field.is_choice() => {
                let numeric = field != ConfigField::Roots;
                if !numeric || c.is_ascii_digit() {
                    self.values[field as usize].push(c);
                }
                ConfigOutcome::None
            }
            KeyCode::Enter => match self.build() {
                Ok(c) => ConfigOutcome::Save(Box::new(c)),
                Err(e) => {
                    self.error = Some(e);
                    ConfigOutcome::None
                }
            },
            _ => ConfigOutcome::None,
        }
    }

    fn build(&self) -> Result<Config, String> {
        let lang = self.language;
        let num = |f: ConfigField, min: u64| -> Result<u64, String> {
            let v: u64 = self.values[f as usize].trim().parse().map_err(|_| i18n::fill(i18n::t(lang, Key::CfgErrNumber), &[f.label(lang)]))?;
            if v < min {
                return Err(i18n::fill(i18n::t(lang, Key::CfgErrMin), &[f.label(lang), &min.to_string()]));
            }
            Ok(v)
        };
        let mut c = self.base.clone();
        c.workspace.roots = self.values[0].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        c.workspace.max_depth = num(ConfigField::MaxDepth, 0)? as usize;
        c.poll.status_secs = num(ConfigField::StatusSecs, 1)?;
        c.poll.auto_fetch_mins = num(ConfigField::AutoFetch, 0)?;
        c.git.log_limit = num(ConfigField::LogLimit, 10)? as usize;
        c.git.network_timeout_secs = num(ConfigField::NetTimeout, 5)?;
        c.ui.language = lang;
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parcial_completa_con_defaults() {
        let c: Config = toml::from_str("[workspace]\nroots = [\"~/x\"]\n").unwrap();
        assert_eq!(c.workspace.max_depth, 3);
        assert_eq!(c.poll.status_secs, 2);
        assert_eq!(c.workspace.roots, vec!["~/x"]);
    }

    #[test]
    fn el_ejemplo_es_valido() {
        let c: Config = toml::from_str(include_str!("../config/voxrepository.example.toml")).unwrap();
        assert_eq!(c.workspace.roots, vec!["~/CodeDev"]);
    }

    #[test]
    fn roundtrip_toml() {
        let c = Config::default();
        let s = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
    }
}
