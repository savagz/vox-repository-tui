//! Marcadores de conflicto (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`):
//! parseo por bloques y reescritura con la elección de cada uno.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Choice {
    #[default]
    Unresolved,
    Ours,
    Theirs,
    /// Ambos lados: primero el nuestro, después el de ellos.
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Block {
    pub ours_label: String,
    pub theirs_label: String,
    pub ours: Vec<String>,
    pub base: Vec<String>,
    pub theirs: Vec<String>,
    /// Líneas de marcador originales, para reescribir intacto lo no resuelto.
    markers: [String; 4],
    has_base: bool,
    pub choice: Choice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(Vec<String>),
    Conflict(Block),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Ours,
    Base,
    Theirs,
}

/// Separa el archivo en texto y bloques de conflicto. Las líneas conservan su
/// fin de línea para reescribir el archivo byte a byte. Un bloque sin cerrar
/// (archivo truncado) se devuelve como texto.
pub fn parse(text: &str) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let mut block: Option<(Block, Part)> = None;
    let mut raw_block: Vec<String> = Vec::new();

    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        match block.as_mut() {
            None => {
                if let Some(label) = marker(bare, '<') {
                    if !plain.is_empty() {
                        out.push(Segment::Text(std::mem::take(&mut plain)));
                    }
                    let mut b = Block { ours_label: label, ..Block::default() };
                    b.markers[0] = line.to_string();
                    raw_block = vec![line.to_string()];
                    block = Some((b, Part::Ours));
                } else {
                    plain.push(line.to_string());
                }
            }
            Some((b, part)) => {
                raw_block.push(line.to_string());
                let mut closed = false;
                if *part == Part::Ours && marker(bare, '|').is_some() {
                    b.markers[1] = line.to_string();
                    b.has_base = true;
                    *part = Part::Base;
                } else if *part != Part::Theirs && bare == "=======" {
                    b.markers[2] = line.to_string();
                    *part = Part::Theirs;
                } else if *part == Part::Theirs && let Some(label) = marker(bare, '>') {
                    b.markers[3] = line.to_string();
                    b.theirs_label = label;
                    closed = true;
                } else {
                    match part {
                        Part::Ours => b.ours.push(line.to_string()),
                        Part::Base => b.base.push(line.to_string()),
                        Part::Theirs => b.theirs.push(line.to_string()),
                    }
                }
                if closed && let Some((done, _)) = block.take() {
                    out.push(Segment::Conflict(done));
                    raw_block.clear();
                }
            }
        }
    }
    if block.is_some() {
        plain.extend(raw_block);
    }
    if !plain.is_empty() {
        out.push(Segment::Text(plain));
    }
    out
}

/// `<<<<<<< etiqueta` → `Some("etiqueta")`. Exactamente siete caracteres.
fn marker(line: &str, c: char) -> Option<String> {
    let prefix: String = std::iter::repeat_n(c, 7).collect();
    let rest = line.strip_prefix(&prefix)?;
    if rest.starts_with(c) {
        return None;
    }
    if rest.is_empty() || rest.starts_with(' ') {
        Some(rest.trim().to_string())
    } else {
        None
    }
}

pub fn render(segments: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segments {
        match seg {
            Segment::Text(lines) => lines.iter().for_each(|l| out.push_str(l)),
            Segment::Conflict(b) => match b.choice {
                Choice::Ours => b.ours.iter().for_each(|l| out.push_str(l)),
                Choice::Theirs => b.theirs.iter().for_each(|l| out.push_str(l)),
                Choice::Both => {
                    b.ours.iter().for_each(|l| out.push_str(l));
                    b.theirs.iter().for_each(|l| out.push_str(l));
                }
                Choice::Unresolved => {
                    out.push_str(&b.markers[0]);
                    b.ours.iter().for_each(|l| out.push_str(l));
                    if b.has_base {
                        out.push_str(&b.markers[1]);
                        b.base.iter().for_each(|l| out.push_str(l));
                    }
                    out.push_str(&b.markers[2]);
                    b.theirs.iter().for_each(|l| out.push_str(l));
                    out.push_str(&b.markers[3]);
                }
            },
        }
    }
    out
}

pub fn blocks(segments: &[Segment]) -> impl Iterator<Item = &Block> {
    segments.iter().filter_map(|s| match s {
        Segment::Conflict(b) => Some(b),
        Segment::Text(_) => None,
    })
}

pub fn block_mut(segments: &mut [Segment], index: usize) -> Option<&mut Block> {
    segments
        .iter_mut()
        .filter_map(|s| match s {
            Segment::Conflict(b) => Some(b),
            Segment::Text(_) => None,
        })
        .nth(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const FILE: &str = "uno\n<<<<<<< HEAD\nnuestro\n=======\nde ellos\n>>>>>>> feat\ndos\n<<<<<<< HEAD\na\n||||||| base\nb\n=======\nc\n>>>>>>> feat\n";

    #[test]
    fn sin_elegir_reescribe_igual() {
        let segs = parse(FILE);
        assert_eq!(blocks(&segs).count(), 2);
        assert_eq!(render(&segs), FILE);
    }

    #[test]
    fn eleccion_por_bloque() {
        let mut segs = parse(FILE);
        block_mut(&mut segs, 0).unwrap().choice = Choice::Theirs;
        block_mut(&mut segs, 1).unwrap().choice = Choice::Both;
        assert_eq!(render(&segs), "uno\nde ellos\ndos\na\nc\n");
        let b = blocks(&segs).nth(1).unwrap();
        assert_eq!(b.base, vec!["b\n".to_string()]);
        assert_eq!(b.theirs_label, "feat");
    }

    #[test]
    fn bloque_sin_cerrar_queda_como_texto() {
        let t = "a\n<<<<<<< HEAD\nx\n";
        let segs = parse(t);
        assert_eq!(blocks(&segs).count(), 0);
        assert_eq!(render(&segs), t);
    }
}
