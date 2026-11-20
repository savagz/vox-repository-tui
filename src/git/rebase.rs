//! Lista de tareas del rebase interactivo. La TUI arma el plan completo y se
//! lo entrega a git como editor de secuencia: git nunca abre un editor.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl Action {
    pub fn word(self) -> &'static str {
        match self {
            Action::Pick => "pick",
            Action::Reword => "reword",
            Action::Edit => "edit",
            Action::Squash => "squash",
            Action::Fixup => "fixup",
            Action::Drop => "drop",
        }
    }

    /// Necesita un commit anterior dentro del plan donde fusionarse.
    pub fn melds(self) -> bool {
        matches!(self, Action::Squash | Action::Fixup)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub action: Action,
    pub hash: String,
    pub subject: String,
    /// Mensaje nuevo para `Reword`; si falta se conserva el original.
    pub message: Option<String>,
}

impl Item {
    pub fn pick(hash: impl Into<String>, subject: impl Into<String>) -> Self {
        Self { action: Action::Pick, hash: hash.into(), subject: subject.into(), message: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    Empty,
    /// El primer commit que sobrevive no puede ser squash/fixup.
    MeldFirst,
}

/// Valida el plan (orden de aplicación: el más viejo primero).
pub fn validate(items: &[Item]) -> Result<(), PlanError> {
    let mut alive = items.iter().filter(|i| i.action != Action::Drop);
    match alive.next() {
        None if items.is_empty() => Err(PlanError::Empty),
        None => Ok(()),
        Some(first) if first.action.melds() => Err(PlanError::MeldFirst),
        Some(_) => Ok(()),
    }
}

/// Texto del todo para git. Un `Reword` con mensaje nuevo se escribe como
/// `pick` seguido de un `exec` que enmienda el mensaje leyendo `msg_path(i)`:
/// así git no necesita abrir un editor en medio del rebase.
pub fn todo(items: &[Item], msg_path: impl Fn(usize) -> String) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        match (item.action, &item.message) {
            (Action::Reword, Some(_)) => {
                out.push_str(&format!("pick {} {}\n", item.hash, item.subject));
                out.push_str(&format!(
                    "exec git commit --amend --only --allow-empty --cleanup=strip -F {}\n",
                    super::sh_quote(&msg_path(i))
                ));
            }
            (Action::Reword, None) => out.push_str(&format!("pick {} {}\n", item.hash, item.subject)),
            (action, _) => out.push_str(&format!("{} {} {}\n", action.word(), item.hash, item.subject)),
        }
    }
    out
}

/// Edición rápida sobre la historia, sin abrir el planificador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickEdit {
    Reword { hash: String, message: String },
    /// Fusiona el commit con el anterior (más viejo) conservando ambos mensajes.
    Squash { hash: String },
    /// Igual que `Squash` pero descarta el mensaje del commit.
    Fixup { hash: String },
    Drop { hash: String },
    /// Lo mueve un lugar hacia lo más nuevo.
    MoveUp { hash: String },
    /// Lo mueve un lugar hacia lo más viejo.
    MoveDown { hash: String },
}

impl QuickEdit {
    pub fn hash(&self) -> &str {
        match self {
            QuickEdit::Reword { hash, .. }
            | QuickEdit::Squash { hash }
            | QuickEdit::Fixup { hash }
            | QuickEdit::Drop { hash }
            | QuickEdit::MoveUp { hash }
            | QuickEdit::MoveDown { hash } => hash,
        }
    }

    /// Cuántos ancestros del commit hay que incluir en el plan: squash y
    /// mover hacia abajo tocan también al padre.
    pub fn depth(&self) -> usize {
        match self {
            QuickEdit::Squash { .. } | QuickEdit::Fixup { .. } | QuickEdit::MoveDown { .. } => 2,
            _ => 1,
        }
    }

    /// Aplica la edición sobre la lista (más viejo primero). `None` si el
    /// commit no está o el movimiento no es posible.
    pub fn apply(&self, items: &mut [Item]) -> Option<()> {
        let idx = items.iter().position(|i| i.hash.starts_with(self.hash()) || self.hash().starts_with(&i.hash))?;
        match self {
            QuickEdit::Reword { message, .. } => {
                items[idx].action = Action::Reword;
                items[idx].message = Some(message.clone());
            }
            QuickEdit::Squash { .. } if idx > 0 => items[idx].action = Action::Squash,
            QuickEdit::Fixup { .. } if idx > 0 => items[idx].action = Action::Fixup,
            QuickEdit::Squash { .. } | QuickEdit::Fixup { .. } => return None,
            QuickEdit::Drop { .. } => items[idx].action = Action::Drop,
            QuickEdit::MoveUp { .. } if idx + 1 < items.len() => items.swap(idx, idx + 1),
            QuickEdit::MoveDown { .. } if idx > 0 => items.swap(idx, idx - 1),
            QuickEdit::MoveUp { .. } | QuickEdit::MoveDown { .. } => return None,
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn plan() -> Vec<Item> {
        vec![Item::pick("a1", "uno"), Item::pick("b2", "dos"), Item::pick("c3", "tres")]
    }

    #[test]
    fn todo_con_reword_usa_exec() {
        let mut items = plan();
        items[1].action = Action::Reword;
        items[1].message = Some("nuevo".into());
        items[2].action = Action::Fixup;
        let t = todo(&items, |i| format!("/tmp/msg-{i}"));
        assert_eq!(
            t,
            "pick a1 uno\npick b2 dos\nexec git commit --amend --only --allow-empty --cleanup=strip -F '/tmp/msg-1'\nfixup c3 tres\n"
        );
    }

    #[test]
    fn validacion() {
        let mut items = plan();
        assert_eq!(validate(&items), Ok(()));
        items[0].action = Action::Drop;
        items[1].action = Action::Squash;
        assert_eq!(validate(&items), Err(PlanError::MeldFirst));
        assert_eq!(validate(&[]), Err(PlanError::Empty));
    }

    #[test]
    fn ediciones_rapidas() {
        let mut items = plan();
        QuickEdit::MoveUp { hash: "a1".into() }.apply(&mut items).unwrap();
        assert_eq!(items[1].hash, "a1");
        assert!(QuickEdit::MoveUp { hash: "c3".into() }.apply(&mut items).is_none());
        let mut items = plan();
        assert!(QuickEdit::Squash { hash: "a1".into() }.apply(&mut items).is_none());
        QuickEdit::Squash { hash: "c3".into() }.apply(&mut items).unwrap();
        assert_eq!(items[2].action, Action::Squash);
    }
}
