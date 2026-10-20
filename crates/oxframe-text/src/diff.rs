//! Diff of two code strings for a token tween.
//!
//! Lines are matched first, then the highlight tokens of a changed line.
//! The result is a pure edit script: drawing and time live elsewhere.

use crate::{Lang, Token, highlight};

/// One highlight token, split at whitespace so a renamed word does not
/// take the spaces around it with it. The color is still the highlighter's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Source text of this piece.
    pub text: String,
    /// Highlight kind, shared by every character of `text`.
    pub token: Token,
}

/// What happened to one piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceEdit {
    /// Present in both lines, in the same order.
    Equal(Piece),
    /// Only in the old line.
    Delete(Piece),
    /// Only in the new line.
    Insert(Piece),
}

/// What happened to one line. Indexes are 0-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEdit {
    /// The line text is unchanged.
    Equal {
        /// Index in the old source.
        old: usize,
        /// Index in the new source.
        new: usize,
        /// The line, without its newline.
        line: String,
    },
    /// The line exists only in the old source.
    Delete {
        /// Index in the old source.
        old: usize,
        /// The line, without its newline.
        line: String,
    },
    /// The line exists only in the new source.
    Insert {
        /// Index in the new source.
        new: usize,
        /// The line, without its newline.
        line: String,
    },
    /// Both sides have a line here, and their text differs.
    Change {
        /// Index in the old source.
        old: usize,
        /// Index in the new source.
        new: usize,
        /// Token edits inside the line.
        pieces: Vec<PieceEdit>,
    },
}

/// Edit script from one source to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDiff {
    /// Lines in visual order: equal, deleted and inserted lines interleaved
    /// so a deleted line sits where it left and an inserted line where it arrives.
    pub lines: Vec<LineEdit>,
}

/// A place in a source. Lines are 1-based, columns are 0-based characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loc {
    /// Line, starting at 1.
    pub line: usize,
    /// Character column, starting at 0.
    pub col: usize,
}

/// Why a source edit could not be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError(pub String);

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lines of `src`, without their newlines. A trailing newline does not add
/// an empty line; a blank line in the middle does.
pub fn code_lines(src: &str) -> Vec<&str> {
    if src.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = src.split('\n').collect();
    if src.ends_with('\n') {
        lines.pop();
    }
    lines
}

/// Diff `from` into `to`. Highlight tokens come from `lang`.
pub fn diff_code(from: &str, to: &str, lang: Lang) -> CodeDiff {
    let a = code_lines(from);
    let b = code_lines(to);
    let matches = lcs(&a, &b);
    let mut lines = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in matches {
        push_hunk(&mut lines, &a, &b, [i, j, mi, mj], lang);
        lines.push(LineEdit::Equal {
            old: mi,
            new: mj,
            line: a[mi].to_owned(),
        });
        i = mi + 1;
        j = mj + 1;
    }
    push_hunk(&mut lines, &a, &b, [i, j, a.len(), b.len()], lang);
    CodeDiff { lines }
}

/// Where the focused old lines land. `(0, 0)` stays "everything".
/// `None` when every focused line was deleted.
pub fn map_focus(diff: &CodeDiff, first: usize, last: usize) -> Option<(usize, usize)> {
    if first == 0 && last == 0 {
        return Some((0, 0));
    }
    let (lo, hi) = if first <= last {
        (first, last)
    } else {
        (last, first)
    };
    let mut news = Vec::new();
    for line in &diff.lines {
        let (old, new) = match line {
            LineEdit::Equal { old, new, .. } | LineEdit::Change { old, new, .. } => (*old, *new),
            LineEdit::Delete { .. } | LineEdit::Insert { .. } => continue,
        };
        let n = old + 1;
        if n >= lo && n <= hi {
            news.push(new + 1);
        }
    }
    Some((*news.iter().min()?, *news.iter().max()?))
}

/// Inserts `text` at `line` (1-based) and `col` (0-based characters).
///
/// # Errors
///
/// When the line or column is outside the source. The line just after the
/// last one, column 0, means the end of the source.
pub fn insert_at(src: &str, line: usize, col: usize, text: &str) -> Result<String, EditError> {
    replace_range(src, Loc { line, col }, Loc { line, col }, text)
}

/// Removes the half-open range `start..end`.
///
/// # Errors
///
/// When a point is outside the source, or `end` is before `start`.
pub fn remove_range(src: &str, start: Loc, end: Loc) -> Result<String, EditError> {
    replace_range(src, start, end, "")
}

/// Replaces the half-open range `start..end` with `text`.
///
/// # Errors
///
/// When a point is outside the source, or `end` is before `start`.
pub fn replace_range(src: &str, start: Loc, end: Loc, text: &str) -> Result<String, EditError> {
    let a = point(src, start)?;
    let b = point(src, end)?;
    if b < a {
        return Err(EditError(format!(
            "the range ends at line {} column {} before it starts at line {} column {}",
            end.line, end.col, start.line, start.col
        )));
    }
    let mut out = String::with_capacity(src.len() - (b - a) + text.len());
    out.push_str(&src[..a]);
    out.push_str(text);
    out.push_str(&src[b..]);
    Ok(out)
}

fn push_hunk(out: &mut Vec<LineEdit>, old: &[&str], new: &[&str], at: [usize; 4], lang: Lang) {
    let [i, j, mi, mj] = at;
    let (del, ins) = (mi - i, mj - j);
    let paired = del.min(ins);
    for k in 0..paired {
        out.push(LineEdit::Change {
            old: i + k,
            new: j + k,
            pieces: piece_diff(old[i + k], new[j + k], lang),
        });
    }
    for k in paired..del {
        out.push(LineEdit::Delete {
            old: i + k,
            line: old[i + k].to_owned(),
        });
    }
    for k in paired..ins {
        out.push(LineEdit::Insert {
            new: j + k,
            line: new[j + k].to_owned(),
        });
    }
}

/// Highlight pieces of one line, split at whitespace.
pub fn line_pieces(line: &str, lang: Lang) -> Vec<Piece> {
    pieces(line, lang)
}

fn piece_diff(old: &str, new: &str, lang: Lang) -> Vec<PieceEdit> {
    let a = pieces(old, lang);
    let b = pieces(new, lang);
    let matches = lcs(&a, &b);
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (mi, mj) in matches {
        for p in a.iter().take(mi).skip(i) {
            out.push(PieceEdit::Delete(p.clone()));
        }
        for p in b.iter().take(mj).skip(j) {
            out.push(PieceEdit::Insert(p.clone()));
        }
        out.push(PieceEdit::Equal(a[mi].clone()));
        i = mi + 1;
        j = mj + 1;
    }
    for p in a.iter().skip(i) {
        out.push(PieceEdit::Delete(p.clone()));
    }
    for p in b.iter().skip(j) {
        out.push(PieceEdit::Insert(p.clone()));
    }
    out
}

fn pieces(line: &str, lang: Lang) -> Vec<Piece> {
    let mut out = Vec::new();
    for (range, token) in highlight(line, lang) {
        let mut rest = &line[range];
        while !rest.is_empty() {
            let ws = rest.chars().next().is_some_and(char::is_whitespace);
            let mut end = 0;
            for c in rest.chars() {
                if c.is_whitespace() != ws {
                    break;
                }
                end += c.len_utf8();
            }
            out.push(Piece {
                text: rest[..end].to_owned(),
                token,
            });
            rest = &rest[end..];
        }
    }
    out
}

fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

fn point(src: &str, at: Loc) -> Result<usize, EditError> {
    if at.line == 0 {
        return Err(EditError(
            "line 0 is not a line: lines start at 1, columns start at 0".into(),
        ));
    }
    let lines = code_lines(src);
    if at.line == lines.len() + 1 {
        if at.col == 0 {
            return Ok(src.len());
        }
        return Err(EditError(format!(
            "line {} is past the end of the code, which has {} lines",
            at.line,
            lines.len()
        )));
    }
    if at.line > lines.len() {
        return Err(EditError(format!(
            "line {} is past the end of the code, which has {} lines",
            at.line,
            lines.len()
        )));
    }
    let row = lines[at.line - 1];
    let chars = row.chars().count();
    if at.col > chars {
        return Err(EditError(format!(
            "column {} is past the end of line {}, which has {chars} characters",
            at.col, at.line
        )));
    }
    let mut byte = 0;
    for (i, line) in lines.iter().enumerate() {
        if i + 1 == at.line {
            byte += line.chars().take(at.col).map(char::len_utf8).sum::<usize>();
            return Ok(byte);
        }
        byte += line.len() + 1;
    }
    Ok(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equals(diff: &CodeDiff) -> Vec<&str> {
        diff.lines
            .iter()
            .filter_map(|l| match l {
                LineEdit::Equal { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect()
    }

    fn inserted(diff: &CodeDiff) -> Vec<&str> {
        diff.lines
            .iter()
            .filter_map(|l| match l {
                LineEdit::Insert { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect()
    }

    fn deleted(diff: &CodeDiff) -> Vec<&str> {
        diff.lines
            .iter()
            .filter_map(|l| match l {
                LineEdit::Delete { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn edit_at_start_middle_and_end() {
        let start = diff_code("b\nc\n", "a\nb\nc\n", Lang::Rust);
        assert_eq!(inserted(&start), ["a"]);
        assert_eq!(equals(&start), ["b", "c"]);

        let middle = diff_code("a\nb\nc\n", "a\nB\nc\n", Lang::Rust);
        assert_eq!(equals(&middle), ["a", "c"]);
        assert!(matches!(
            &middle.lines[1],
            LineEdit::Change { old: 1, new: 1, .. }
        ));

        let end = diff_code("a\nb\n", "a\nb\nc\n", Lang::Rust);
        assert_eq!(inserted(&end), ["c"]);
        assert_eq!(equals(&end), ["a", "b"]);
    }

    #[test]
    fn empty_line_is_its_own_row() {
        let diff = diff_code("a\nb\n", "a\n\nb\n", Lang::Rust);
        assert_eq!(inserted(&diff), [""]);
        assert_eq!(equals(&diff), ["a", "b"]);
        assert!(deleted(&diff).is_empty());
    }

    #[test]
    fn moving_a_block_deletes_it_and_inserts_it() {
        let from = "A\nB\nC\nX\nY\nZ\n";
        let to = "X\nY\nZ\nA\nB\nC\n";
        for lang in [
            Lang::Rust,
            Lang::Js,
            Lang::Python,
            Lang::Json,
            Lang::Toml,
            Lang::Shell,
        ] {
            let diff = diff_code(from, to, lang);
            let gone = deleted(&diff);
            let arrived = inserted(&diff);
            assert_eq!(gone, arrived, "{lang:?}");
            assert_eq!(gone.len(), 3, "{lang:?}");
            assert_eq!(equals(&diff).len(), 3, "{lang:?}");
        }
    }

    #[test]
    fn a_renamed_word_leaves_its_keyword() {
        let cases = [
            (
                Lang::Rust,
                "fn main() {\n    let x = 1;\n}\n",
                "fn main() {\n    let y = 1;\n}\n",
                "let",
            ),
            (
                Lang::Js,
                "function f() {\n  const x = 1;\n}\n",
                "function f() {\n  const y = 1;\n}\n",
                "const",
            ),
            (
                Lang::Python,
                "def f():\n    return x\n",
                "def f():\n    return y\n",
                "return",
            ),
            (
                Lang::Json,
                "{\n  \"a\": 1\n}\n",
                "{\n  \"a\": 2\n}\n",
                "\"a\"",
            ),
            (Lang::Toml, "name = \"a\"\n", "name = \"b\"\n", "name"),
            (Lang::Shell, "echo hello\n", "echo world\n", "echo"),
        ];
        for (lang, from, to, word) in cases {
            let diff = diff_code(from, to, lang);
            let kept: Vec<&str> = diff
                .lines
                .iter()
                .filter_map(|l| match l {
                    LineEdit::Change { pieces, .. } => Some(pieces),
                    _ => None,
                })
                .flatten()
                .filter_map(|p| match p {
                    PieceEdit::Equal(p) => Some(p.text.as_str()),
                    _ => None,
                })
                .collect();
            assert!(
                kept.contains(&word),
                "{lang:?} dropped {word:?} from {kept:?}"
            );
            assert!(!kept.contains(&"x") || lang == Lang::Toml, "{lang:?}");
        }
    }

    #[test]
    fn rust_ident_change_does_not_redraw_the_spaces() {
        let diff = diff_code("let x = 1;", "let y = 1;", Lang::Rust);
        let LineEdit::Change { pieces, .. } = &diff.lines[0] else {
            panic!("expected a change, got {diff:?}");
        };
        let texts: Vec<_> = pieces
            .iter()
            .map(|p| match p {
                PieceEdit::Equal(p) => ("=", p.text.as_str()),
                PieceEdit::Delete(p) => ("-", p.text.as_str()),
                PieceEdit::Insert(p) => ("+", p.text.as_str()),
            })
            .collect();
        assert!(texts.contains(&("=", "let")), "{texts:?}");
        assert!(texts.contains(&("-", "x")), "{texts:?}");
        assert!(texts.contains(&("+", "y")), "{texts:?}");
        assert!(texts.contains(&("=", " ")), "{texts:?}");
        assert!(texts.contains(&("=", "1")), "{texts:?}");
    }

    #[test]
    fn focus_follows_lines_that_shift() {
        let diff = diff_code("a\nb\nc\n", "NEW\na\nb\nc\n", Lang::Rust);
        assert_eq!(map_focus(&diff, 2, 2), Some((3, 3)));
        assert_eq!(map_focus(&diff, 0, 0), Some((0, 0)));
        let gone = diff_code("a\nb\n", "b\n", Lang::Rust);
        assert_eq!(map_focus(&gone, 1, 1), None);
    }

    #[test]
    fn insert_remove_and_replace_use_lines_and_columns() {
        let src = "aaaa\nbbbb\ncccc\n";
        assert_eq!(
            insert_at(src, 2, 0, "X\n").unwrap(),
            "aaaa\nX\nbbbb\ncccc\n"
        );
        assert_eq!(
            remove_range(src, Loc { line: 2, col: 0 }, Loc { line: 3, col: 0 }).unwrap(),
            "aaaa\ncccc\n"
        );
        assert_eq!(
            replace_range(src, Loc { line: 2, col: 1 }, Loc { line: 2, col: 3 }, "YZ").unwrap(),
            "aaaa\nbYZb\ncccc\n"
        );
        // One character, even though "й" is two bytes.
        assert_eq!(insert_at("й\n", 1, 1, "!").unwrap(), "й!\n");
        let err = insert_at(src, 0, 0, "z").unwrap_err();
        assert!(err.0.contains("start at 1"), "{err}");
        let err = remove_range(src, Loc { line: 9, col: 0 }, Loc { line: 9, col: 0 }).unwrap_err();
        assert!(err.0.contains("3 lines"), "{err}");
    }
}
