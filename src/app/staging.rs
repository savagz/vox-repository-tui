//! Cursor sobre el diff de un archivo para indexar por hunk o por línea.

use crate::git::patch::{self, FileDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Header(usize),
    Hunk(usize),
    /// (hunk, línea dentro del hunk)
    Line(usize, usize),
}

#[derive(Debug, Clone)]
pub struct Staging {
    pub path: String,
    /// El diff es del índice (HEAD→índice) y no del árbol.
    pub staged: bool,
    pub untracked: bool,
    pub diff: FileDiff,
    pub rows: Vec<Row>,
    pub cursor: usize,
    /// Inicio de la selección de rango (`v`).
    pub anchor: Option<usize>,
}

impl Staging {
    pub fn new(path: String, staged: bool, untracked: bool, text: &str) -> Self {
        let diff = patch::parse(text);
        let mut rows: Vec<Row> = (0..diff.header.len()).map(Row::Header).collect();
        for (h, hunk) in diff.hunks.iter().enumerate() {
            rows.push(Row::Hunk(h));
            rows.extend((0..hunk.lines.len()).map(|i| Row::Line(h, i)));
        }
        let mut s = Self { path, staged, untracked, diff, rows, cursor: 0, anchor: None };
        s.cursor = s.first_change().unwrap_or(0);
        s
    }

    pub fn text(&self, row: Row) -> String {
        match row {
            Row::Header(i) => self.diff.header[i].clone(),
            Row::Hunk(h) => self.diff.hunks[h].header(),
            Row::Line(h, i) => self.diff.hunks[h].lines[i].clone(),
        }
    }

    pub fn is_change(&self, idx: usize) -> bool {
        match self.rows.get(idx) {
            Some(Row::Line(h, i)) => patch::is_change(&self.diff.hunks[*h].lines[*i]),
            _ => false,
        }
    }

    pub fn has_changes(&self) -> bool {
        (0..self.rows.len()).any(|i| self.is_change(i))
    }

    fn first_change(&self) -> Option<usize> {
        (0..self.rows.len()).find(|&i| self.is_change(i))
    }

    /// Mueve el cursor a la línea de cambio más cercana en la dirección dada.
    pub fn move_cursor(&mut self, delta: isize) {
        let mut idx = self.cursor as isize;
        let steps = delta.unsigned_abs();
        let dir = delta.signum();
        for _ in 0..steps {
            let mut probe = idx + dir;
            while probe >= 0 && (probe as usize) < self.rows.len() && !self.is_change(probe as usize) {
                probe += dir;
            }
            if probe < 0 || probe as usize >= self.rows.len() {
                break;
            }
            idx = probe;
        }
        self.cursor = idx as usize;
    }

    /// Salta al primer cambio del hunk siguiente/anterior.
    pub fn jump_hunk(&mut self, forward: bool) {
        let Some(cur) = self.hunk_at(self.cursor) else { return };
        let target = if forward { cur + 1 } else { cur.wrapping_sub(1) };
        if target >= self.diff.hunks.len() {
            return;
        }
        if let Some(i) = (0..self.rows.len()).find(|&i| matches!(self.rows[i], Row::Line(h, _) if h == target) && self.is_change(i)) {
            self.cursor = i;
            self.anchor = None;
        }
    }

    /// Tras recargar el diff, vuelve a una posición equivalente.
    pub fn restore_cursor(&mut self, prev: usize) {
        if self.rows.is_empty() {
            self.cursor = 0;
            return;
        }
        self.cursor = prev.min(self.rows.len() - 1);
        if !self.is_change(self.cursor) {
            let before = self.cursor;
            self.move_cursor(1);
            if self.cursor == before {
                self.move_cursor(-1);
            }
        }
    }

    pub fn hunk_at(&self, idx: usize) -> Option<usize> {
        match self.rows.get(idx)? {
            Row::Hunk(h) | Row::Line(h, _) => Some(*h),
            Row::Header(_) => None,
        }
    }

    /// Rango de filas seleccionado (inclusive), acotado al hunk del cursor.
    pub fn range(&self) -> (usize, usize) {
        let anchor = self.anchor.unwrap_or(self.cursor);
        let (a, b) = if anchor <= self.cursor { (anchor, self.cursor) } else { (self.cursor, anchor) };
        let hunk = self.hunk_at(self.cursor);
        let a = (a..=b).find(|&i| self.hunk_at(i) == hunk).unwrap_or(self.cursor);
        let b = (a..=b).rev().find(|&i| self.hunk_at(i) == hunk).unwrap_or(self.cursor);
        (a, b)
    }

    /// (hunk, líneas elegidas) para armar el parche de la selección actual.
    pub fn selection(&self) -> Option<(usize, Vec<usize>)> {
        let hunk = self.hunk_at(self.cursor)?;
        let (a, b) = self.range();
        let lines: Vec<usize> = (a..=b)
            .filter(|&i| self.is_change(i))
            .filter_map(|i| match self.rows[i] {
                Row::Line(h, l) if h == hunk => Some(l),
                _ => None,
            })
            .collect();
        (!lines.is_empty()).then_some((hunk, lines))
    }

    pub fn in_range(&self, idx: usize) -> bool {
        if self.anchor.is_none() {
            return idx == self.cursor;
        }
        let (a, b) = self.range();
        idx >= a && idx <= b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n@@ -9,2 +9,2 @@\n x\n-y\n+Y\n";

    #[test]
    fn cursor_salta_contexto() {
        let mut s = Staging::new("f".into(), false, false, DIFF);
        assert_eq!(s.text(s.rows[s.cursor]), "-b");
        s.move_cursor(1);
        assert_eq!(s.text(s.rows[s.cursor]), "+B");
        s.move_cursor(1);
        assert_eq!(s.text(s.rows[s.cursor]), "-y");
        s.move_cursor(5);
        assert_eq!(s.text(s.rows[s.cursor]), "+Y");
        s.jump_hunk(false);
        assert_eq!(s.text(s.rows[s.cursor]), "-b");
    }

    #[test]
    fn seleccion_acotada_al_hunk() {
        let mut s = Staging::new("f".into(), false, false, DIFF);
        s.anchor = Some(s.cursor);
        s.move_cursor(3);
        // El cursor quedó en el segundo hunk: la selección sólo toma ese.
        let (h, lines) = s.selection().unwrap();
        assert_eq!(h, 1);
        assert_eq!(lines, vec![1, 2]);
    }
}
