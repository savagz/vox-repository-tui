//! Diff unificado de un archivo: parseo en hunks y armado de parches parciales
//! (un hunk o un rango de líneas) para `git apply`.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiff {
    pub header: Vec<String>,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    /// Texto después del segundo `@@` (la función que git detecta).
    pub context: String,
    /// Líneas con su prefijo (' ', '+', '-', '\').
    pub lines: Vec<String>,
}

impl Hunk {
    pub fn header(&self) -> String {
        let (o, n) = counts(&self.lines);
        format!("@@ -{},{} +{},{} @@{}", self.old_start, o, self.new_start, n, self.context)
    }
}

fn counts(lines: &[String]) -> (u32, u32) {
    let mut old = 0;
    let mut new = 0;
    for l in lines {
        match l.as_bytes().first() {
            Some(b' ') => {
                old += 1;
                new += 1;
            }
            Some(b'-') => old += 1,
            Some(b'+') => new += 1,
            _ => {}
        }
    }
    (old, new)
}

pub fn is_change(line: &str) -> bool {
    line.starts_with('+') || line.starts_with('-')
}

/// Parsea la salida de `git diff -- <archivo>` (un solo archivo).
pub fn parse(text: &str) -> FileDiff {
    let mut diff = FileDiff::default();
    for line in text.lines() {
        if line.starts_with("@@")
            && let Some(h) = parse_hunk_header(line)
        {
            diff.hunks.push(h);
            continue;
        }
        match diff.hunks.last_mut() {
            Some(h) if !line.starts_with("diff --git") => h.lines.push(line.to_string()),
            _ => {
                if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
                    diff.binary = true;
                }
                diff.header.push(line.to_string());
            }
        }
    }
    diff
}

fn parse_hunk_header(line: &str) -> Option<Hunk> {
    // @@ -a,b +c,d @@ contexto
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, context) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let start = |r: &str| r.split(',').next().and_then(|n| n.parse::<u32>().ok());
    Some(Hunk {
        old_start: start(old)?,
        new_start: start(new)?,
        context: context.to_string(),
        lines: Vec::new(),
    })
}

/// Sentido en que se va a aplicar el parche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    /// `git apply --cached` del diff índice→árbol: agrega al índice.
    Forward,
    /// `git apply --reverse` (con o sin `--cached`): quita del índice (diff
    /// HEAD→índice) o descarta del árbol (diff índice→árbol).
    Reverse,
}

/// Parche con un solo hunk, opcionalmente limitado a algunas líneas (índices
/// dentro de `hunk.lines`). Las líneas no elegidas se neutralizan según el
/// sentido: al aplicar hacia adelante un '-' no elegido sigue existiendo (pasa
/// a contexto) y un '+' no elegido no existe todavía (se omite); en reversa es
/// al revés, porque git compara contra el lado "nuevo" del parche.
pub fn build(diff: &FileDiff, hunk: usize, selected: Option<&[usize]>, mode: Apply) -> Option<String> {
    let h = diff.hunks.get(hunk)?;
    let mut lines: Vec<String> = Vec::with_capacity(h.lines.len());
    let mut any_change = false;
    let mut last_kept = true;
    for (i, line) in h.lines.iter().enumerate() {
        let chosen = selected.is_none_or(|s| s.contains(&i));
        let first = line.as_bytes().first().copied();
        match first {
            Some(b'\\') => {
                if last_kept {
                    lines.push(line.clone());
                }
                continue;
            }
            Some(b'+') | Some(b'-') if chosen => {
                any_change = true;
                lines.push(line.clone());
                last_kept = true;
            }
            Some(b'+') | Some(b'-') => {
                let becomes_context = match mode {
                    Apply::Forward => first == Some(b'-'),
                    Apply::Reverse => first == Some(b'+'),
                };
                if becomes_context {
                    lines.push(format!(" {}", &line[1..]));
                }
                last_kept = becomes_context;
            }
            _ => {
                lines.push(line.clone());
                last_kept = true;
            }
        }
    }
    if !any_change {
        return None;
    }
    let partial = Hunk { lines, ..h.clone() };
    let mut out = String::new();
    for l in &diff.header {
        out.push_str(l);
        out.push('\n');
    }
    out.push_str(&partial.header());
    out.push('\n');
    for l in &partial.lines {
        out.push_str(l);
        out.push('\n');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const DIFF: &str = "diff --git a/f.txt b/f.txt
index 111..222 100644
--- a/f.txt
+++ b/f.txt
@@ -1,4 +1,4 @@ fn x
 a
-b
+B
 c
-d
+D
@@ -10,2 +10,3 @@
 x
+y
 z
";

    #[test]
    fn parsea_hunks() {
        let d = parse(DIFF);
        assert_eq!(d.header.len(), 4);
        assert_eq!(d.hunks.len(), 2);
        assert_eq!(d.hunks[0].context, " fn x");
        assert_eq!(d.hunks[1].old_start, 10);
        assert_eq!(d.hunks[0].header(), "@@ -1,4 +1,4 @@ fn x");
    }

    #[test]
    fn hunk_completo() {
        let d = parse(DIFF);
        let p = build(&d, 1, None, Apply::Forward).unwrap();
        assert!(p.ends_with("@@ -10,2 +10,3 @@\n x\n+y\n z\n"));
    }

    #[test]
    fn lineas_hacia_adelante() {
        let d = parse(DIFF);
        // Sólo "-b" y "+B" (índices 1 y 2): "-d" pasa a contexto, "+D" se omite.
        let p = build(&d, 0, Some(&[1, 2]), Apply::Forward).unwrap();
        assert!(p.ends_with("@@ -1,4 +1,4 @@ fn x\n a\n-b\n+B\n c\n d\n"), "{p}");
    }

    #[test]
    fn lineas_en_reversa() {
        let d = parse(DIFF);
        // En reversa "+D" no elegido pasa a contexto y "-d" se omite.
        let p = build(&d, 0, Some(&[1, 2]), Apply::Reverse).unwrap();
        assert!(p.ends_with("@@ -1,4 +1,4 @@ fn x\n a\n-b\n+B\n c\n D\n"), "{p}");
    }

    #[test]
    fn sin_cambios_elegidos_no_hay_parche() {
        let d = parse(DIFF);
        assert_eq!(build(&d, 0, Some(&[0]), Apply::Forward), None);
    }

    #[test]
    fn marcador_sin_salto_final() {
        let d = parse("--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n");
        let p = build(&d, 0, Some(&[2]), Apply::Forward).unwrap();
        assert!(p.ends_with("@@ -1,1 +1,2 @@\n a\n\\ No newline at end of file\n+b\n\\ No newline at end of file\n"), "{p}");
    }
}
