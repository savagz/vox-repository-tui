//! Overlays modales: formularios, confirmaciones, menús, salida de texto y
//! el editor de mensaje de commit. Cada uno resuelve a un `Outcome` y lleva
//! el `Intent` que se ejecuta al confirmar.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::intent::Intent;
use super::planner::Planner;
use crate::config::ConfigForm;

/// Campo de texto de una línea con cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    pub value: String,
    /// Posición del cursor en caracteres.
    pub cursor: usize,
}

impl TextInput {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self { value, cursor }
    }

    fn byte_at(&self, char_idx: usize) -> usize {
        self.value.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(self.value.len())
    }

    /// Procesa teclas de edición. Devuelve `true` si la consumió.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let len = self.value.chars().count();
        match key.code {
            KeyCode::Char('u') if ctrl => {
                let at = self.byte_at(self.cursor);
                self.value.replace_range(..at, "");
                self.cursor = 0;
            }
            KeyCode::Char('w') if ctrl => {
                let chars: Vec<char> = self.value.chars().collect();
                let mut start = self.cursor;
                while start > 0 && chars[start - 1] == ' ' {
                    start -= 1;
                }
                while start > 0 && chars[start - 1] != ' ' {
                    start -= 1;
                }
                let (a, b) = (self.byte_at(start), self.byte_at(self.cursor));
                self.value.replace_range(a..b, "");
                self.cursor = start;
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = len,
            KeyCode::Char(c) if !ctrl => {
                let at = self.byte_at(self.cursor);
                self.value.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let at = self.byte_at(self.cursor - 1);
                self.value.remove(at);
                self.cursor -= 1;
            }
            KeyCode::Delete if self.cursor < len => {
                let at = self.byte_at(self.cursor);
                self.value.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(len),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = len,
            KeyCode::Backspace | KeyCode::Delete => {}
            _ => return false,
        }
        true
    }

    /// Texto partido en (antes del cursor, carácter bajo el cursor, resto).
    pub fn split(&self) -> (String, String, String) {
        let chars: Vec<char> = self.value.chars().collect();
        let before: String = chars[..self.cursor.min(chars.len())].iter().collect();
        let at: String = chars.get(self.cursor).map(|c| c.to_string()).unwrap_or_else(|| " ".into());
        let after: String = chars.get(self.cursor + 1..).map(|s| s.iter().collect()).unwrap_or_default();
        (before, at, after)
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: String,
    pub input: TextInput,
    /// Si tiene opciones, se elige con ← → en vez de escribirse.
    pub choices: Option<Vec<String>>,
    pub required: bool,
}

impl Field {
    pub fn text(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { label: label.into(), input: TextInput::new(value), choices: None, required: true }
    }

    pub fn optional(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { required: false, ..Self::text(label, value) }
    }

    pub fn choice(label: impl Into<String>, choices: Vec<String>) -> Self {
        let first = choices.first().cloned().unwrap_or_default();
        Self { label: label.into(), input: TextInput::new(first), choices: Some(choices), required: true }
    }
}

#[derive(Debug, Clone)]
pub struct Form {
    pub title: String,
    pub subtitle: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
    pub intent: Intent,
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    /// Texto que hay que tipear para habilitar ENTER (acciones irreversibles).
    pub typed: Option<String>,
    pub input: TextInput,
    pub intent: Intent,
}

#[derive(Debug, Clone)]
pub struct MenuItem {
    pub key: char,
    pub label: String,
    pub intent: Intent,
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub title: String,
    pub items: Vec<MenuItem>,
    pub sel: usize,
}

#[derive(Debug, Clone)]
pub struct Output {
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: u16,
    pub error: bool,
}

/// Editor de mensaje de commit: resumen de una línea y cuerpo multilínea.
#[derive(Debug, Clone)]
pub struct CommitForm {
    pub summary: TextInput,
    pub body: Vec<TextInput>,
    /// 0 = resumen, 1.. = línea del cuerpo (índice + 1).
    pub focus: usize,
    pub amend: bool,
    pub error: Option<String>,
    pub intent: Intent,
}

impl CommitForm {
    pub fn new(message: &str, amend: bool, intent: Intent) -> Self {
        let mut lines = message.lines();
        let summary = TextInput::new(lines.next().unwrap_or(""));
        let rest: Vec<&str> = lines.skip_while(|l| l.trim().is_empty()).collect();
        let mut body: Vec<TextInput> = rest.into_iter().map(TextInput::new).collect();
        if body.is_empty() {
            body.push(TextInput::default());
        }
        Self { summary, body, focus: 0, amend, error: None, intent }
    }

    pub fn message(&self) -> String {
        let body: Vec<&str> = self.body.iter().map(|l| l.value.as_str()).collect();
        let body = body.join("\n");
        let body = body.trim_end();
        if body.trim().is_empty() {
            self.summary.value.trim().to_string()
        } else {
            format!("{}\n\n{}", self.summary.value.trim(), body)
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        self.error = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return Outcome::Cancel,
            KeyCode::Char('s') if ctrl => return self.submit(),
            KeyCode::Enter if self.focus == 0 => return self.submit(),
            KeyCode::Tab => self.focus = if self.focus == 0 { 1 } else { 0 },
            KeyCode::Enter => {
                let idx = self.focus - 1;
                let line = &mut self.body[idx];
                let at = line.value.char_indices().nth(line.cursor).map(|(i, _)| i).unwrap_or(line.value.len());
                let tail = line.value.split_off(at);
                self.body.insert(idx + 1, TextInput { value: tail, cursor: 0 });
                self.focus += 1;
            }
            KeyCode::Up if self.focus > 0 => self.focus -= 1,
            KeyCode::Down if self.focus < self.body.len() => self.focus += 1,
            KeyCode::Backspace if self.focus > 1 && self.body[self.focus - 1].cursor == 0 => {
                let idx = self.focus - 1;
                let removed = self.body.remove(idx);
                let prev = &mut self.body[idx - 1];
                prev.cursor = prev.value.chars().count();
                prev.value.push_str(&removed.value);
                self.focus -= 1;
            }
            _ => {
                let input = if self.focus == 0 { &mut self.summary } else { &mut self.body[self.focus - 1] };
                input.on_key(key);
            }
        }
        Outcome::None
    }

    fn submit(&self) -> Outcome {
        if self.summary.value.trim().is_empty() {
            return Outcome::Invalid;
        }
        Outcome::Submit(vec![self.message()])
    }
}

#[derive(Debug, Clone)]
pub enum Modal {
    Form(Form),
    Confirm(Confirm),
    Menu(Menu),
    Output(Output),
    Commit(CommitForm),
    Planner(Box<Planner>),
    Config(Box<ConfigForm>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    None,
    Cancel,
    /// Confirmado con los valores de los campos.
    Submit(Vec<String>),
    /// Falta un campo obligatorio.
    Invalid,
    /// Se eligió una opción de un menú.
    Pick(usize),
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>, intent: Intent) -> Self {
        Self { title: title.into(), subtitle: String::new(), fields, focus: 0, error: None, intent }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        self.error = None;
        let n = self.fields.len();
        match key.code {
            KeyCode::Esc => Outcome::Cancel,
            KeyCode::Tab | KeyCode::Down => {
                self.focus = (self.focus + 1) % n;
                Outcome::None
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + n - 1) % n;
                Outcome::None
            }
            KeyCode::Enter => {
                if let Some(i) = self.fields.iter().position(|f| f.required && f.input.value.trim().is_empty()) {
                    self.focus = i;
                    return Outcome::Invalid;
                }
                Outcome::Submit(self.fields.iter().map(|f| f.input.value.clone()).collect())
            }
            KeyCode::Left | KeyCode::Right if self.fields[self.focus].choices.is_some() => {
                let field = &mut self.fields[self.focus];
                let choices = field.choices.as_ref().expect("campo con opciones");
                let cur = choices.iter().position(|c| *c == field.input.value).unwrap_or(0);
                let next = if key.code == KeyCode::Right { (cur + 1) % choices.len() } else { (cur + choices.len() - 1) % choices.len() };
                field.input = TextInput::new(choices[next].clone());
                Outcome::None
            }
            _ => {
                let field = &mut self.fields[self.focus];
                if field.choices.is_none() {
                    field.input.on_key(key);
                }
                Outcome::None
            }
        }
    }
}

impl Confirm {
    pub fn new(title: impl Into<String>, message: impl Into<String>, intent: Intent) -> Self {
        Self { title: title.into(), message: message.into(), typed: None, input: TextInput::default(), intent }
    }

    pub fn typed(mut self, expected: impl Into<String>) -> Self {
        self.typed = Some(expected.into());
        self
    }

    pub fn ready(&self) -> bool {
        self.typed.as_ref().is_none_or(|t| *t == self.input.value)
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        match key.code {
            KeyCode::Esc => Outcome::Cancel,
            KeyCode::Enter if self.ready() => Outcome::Submit(Vec::new()),
            KeyCode::Enter => Outcome::Invalid,
            KeyCode::Char('y') | KeyCode::Char('Y') if self.typed.is_none() => Outcome::Submit(Vec::new()),
            KeyCode::Char('n') | KeyCode::Char('N') if self.typed.is_none() => Outcome::Cancel,
            _ => {
                if self.typed.is_some() {
                    self.input.on_key(key);
                }
                Outcome::None
            }
        }
    }
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<MenuItem>) -> Self {
        Self { title: title.into(), items, sel: 0 }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        let n = self.items.len().max(1);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Outcome::Cancel,
            KeyCode::Up | KeyCode::Char('k') => {
                self.sel = (self.sel + n - 1) % n;
                Outcome::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.sel = (self.sel + 1) % n;
                Outcome::None
            }
            KeyCode::Enter => Outcome::Pick(self.sel),
            KeyCode::Char(c) => match self.items.iter().position(|i| i.key == c) {
                Some(i) => Outcome::Pick(i),
                None => Outcome::None,
            },
            _ => Outcome::None,
        }
    }
}

impl Output {
    pub fn new(title: impl Into<String>, text: &str, error: bool) -> Self {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self { title: title.into(), lines, scroll: 0, error }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Outcome {
        let max = self.lines.len().saturating_sub(1) as u16;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll = (self.scroll + 1).min(max),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(15),
            KeyCode::PageDown => self.scroll = (self.scroll + 15).min(max),
            KeyCode::Home | KeyCode::Char('g') => self.scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.scroll = max,
            _ => return Outcome::Cancel,
        }
        Outcome::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    #[test]
    fn text_input_edita_en_el_cursor() {
        let mut t = TextInput::new("ab");
        t.on_key(key(KeyCode::Left));
        t.on_key(key(KeyCode::Char('ñ')));
        assert_eq!(t.value, "añb");
        t.on_key(key(KeyCode::Backspace));
        assert_eq!(t.value, "ab");
        t.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        assert_eq!(t.value, "b");
    }

    #[test]
    fn commit_form_arma_mensaje() {
        let mut f = CommitForm::new("", false, Intent::Commit { amend: false });
        assert_eq!(f.on_key(key(KeyCode::Enter)), Outcome::Invalid);
        for c in "fix: x".chars() {
            f.on_key(key(KeyCode::Char(c)));
        }
        f.on_key(key(KeyCode::Tab));
        for c in "cuerpo".chars() {
            f.on_key(key(KeyCode::Char(c)));
        }
        f.on_key(key(KeyCode::Enter));
        f.on_key(key(KeyCode::Char('2')));
        assert_eq!(f.message(), "fix: x\n\ncuerpo\n2");
        let edit = CommitForm::new("a\n\nb\nc", true, Intent::Commit { amend: true });
        assert_eq!(edit.message(), "a\n\nb\nc");
    }
}
