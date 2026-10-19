//! A small, dependency-free syntax highlighter for code in animations.
//! It favors robustness over completeness: unknown input degrades to plain
//! text, never fails.

use std::ops::Range;

/// Language for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    /// Rust.
    Rust,
    /// JavaScript and TypeScript.
    Js,
    /// Python.
    Python,
    /// JSON.
    Json,
    /// TOML.
    Toml,
    /// Shell scripts.
    Shell,
    /// No highlighting.
    Plain,
}

impl Lang {
    /// From a name or file extension (`"rs"`, `"typescript"`…); unknown names are `Plain`.
    pub fn parse(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Self::Rust,
            "js" | "javascript" | "ts" | "typescript" | "jsx" | "tsx" => Self::Js,
            "py" | "python" => Self::Python,
            "json" => Self::Json,
            "toml" => Self::Toml,
            "sh" | "bash" | "shell" | "zsh" => Self::Shell,
            _ => Self::Plain,
        }
    }
}

/// Kind of a highlighted piece of code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    /// Anything else.
    Plain,
    /// Keyword.
    Keyword,
    /// Type name.
    Type,
    /// Function name.
    Function,
    /// Macro name.
    Macro,
    /// String literal.
    String,
    /// Number literal.
    Number,
    /// Comment.
    Comment,
    /// Attribute or decorator.
    Attribute,
    /// Punctuation or operator.
    Punct,
}

const RUST: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];
const JS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "of",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "yield",
];
const PY: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "self", "True", "try",
    "while", "with", "yield",
];
const SH: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac",
    "function", "return", "export", "local", "echo", "cd",
];

/// Splits `code` into highlighted ranges covering all of it.
pub fn highlight(code: &str, lang: Lang) -> Vec<(Range<usize>, Token)> {
    let b = code.as_bytes();
    let mut out: Vec<(Range<usize>, Token)> = Vec::new();
    let push = |out: &mut Vec<(Range<usize>, Token)>, r: Range<usize>, t: Token| {
        if r.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, lt)) if *lt == t && last.end == r.start => last.end = r.end,
            _ => out.push((r, t)),
        }
    };
    if lang == Lang::Plain {
        push(&mut out, 0..code.len(), Token::Plain);
        return out;
    }
    let keywords: &[&str] = match lang {
        Lang::Rust => RUST,
        Lang::Js => JS,
        Lang::Python => PY,
        Lang::Shell => SH,
        Lang::Json => &["true", "false", "null"],
        Lang::Toml => &["true", "false"],
        Lang::Plain => &[],
    };
    let line_comment: &[&str] = match lang {
        Lang::Rust | Lang::Js => &["//"],
        Lang::Python | Lang::Shell | Lang::Toml => &["#"],
        _ => &[],
    };
    let block_comment = matches!(lang, Lang::Rust | Lang::Js);
    let mut i = 0;
    while i < b.len() {
        let rest = &code[i..];
        let c = b[i];
        // Attributes: #[...] in Rust, @decorator in Python/TS.
        if lang == Lang::Rust && rest.starts_with("#[")
            || lang == Lang::Rust && rest.starts_with("#![")
        {
            let end = rest.find(']').map_or(code.len(), |e| i + e + 1);
            push(&mut out, i..end, Token::Attribute);
            i = end;
            continue;
        }
        if matches!(lang, Lang::Python | Lang::Js) && c == b'@' {
            let end = i
                + 1
                + rest[1..]
                    .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '.'))
                    .unwrap_or(rest.len() - 1);
            push(&mut out, i..end, Token::Attribute);
            i = end;
            continue;
        }
        if let Some(lc) = line_comment.iter().find(|p| rest.starts_with(**p)) {
            let _ = lc;
            let end = rest.find('\n').map_or(code.len(), |e| i + e);
            push(&mut out, i..end, Token::Comment);
            i = end;
            continue;
        }
        if block_comment && rest.starts_with("/*") {
            let end = rest[2..].find("*/").map_or(code.len(), |e| i + 2 + e + 2);
            push(&mut out, i..end, Token::Comment);
            i = end;
            continue;
        }
        // TOML table headers.
        if lang == Lang::Toml && c == b'[' && (i == 0 || b[i - 1] == b'\n') {
            let end = rest.find('\n').map_or(code.len(), |e| i + e);
            push(&mut out, i..end, Token::Type);
            i = end;
            continue;
        }
        if c == b'"' || c == b'\'' || (c == b'`' && lang == Lang::Js) {
            // Rust lifetimes ('a) are not strings.
            if c == b'\'' && lang == Lang::Rust {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j < b.len() && b[j] != b'\'' && j > i + 1 {
                    push(&mut out, i..j, Token::Type);
                    i = j;
                    continue;
                }
            }
            let mut j = i + 1;
            while j < b.len() && b[j] != c {
                if b[j] == b'\\' {
                    j += 1;
                }
                if b[j.min(b.len() - 1)] == b'\n' && c != b'`' && c != b'"' {
                    break;
                }
                j += 1;
            }
            let end = (j + 1).min(b.len());
            push(&mut out, i..end, Token::String);
            i = end;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'.') {
                // `1..2` is a range, not a number.
                if b[j] == b'.' && j + 1 < b.len() && b[j + 1] == b'.' {
                    break;
                }
                j += 1;
            }
            push(&mut out, i..j, Token::Number);
            i = j;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] >= 0x80) {
                j += 1;
            }
            let word = &code[i..j];
            let next = code[j..].chars().find(|ch| !ch.is_whitespace());
            let t = if keywords.contains(&word) {
                Token::Keyword
            } else if lang == Lang::Rust && code[j..].starts_with('!') {
                Token::Macro
            } else if lang == Lang::Json && next == Some(':') {
                Token::Attribute
            } else if next == Some('(') || (lang == Lang::Rust && code[j..].starts_with("::<")) {
                Token::Function
            } else if word.chars().next().is_some_and(char::is_uppercase) {
                Token::Type
            } else if lang == Lang::Toml && next == Some('=') {
                Token::Attribute
            } else {
                Token::Plain
            };
            let end = if t == Token::Macro { j + 1 } else { j };
            push(&mut out, i..end, t);
            i = end;
            continue;
        }
        let ch_len = rest.chars().next().map_or(1, char::len_utf8);
        let t = if c.is_ascii_punctuation() {
            Token::Punct
        } else {
            Token::Plain
        };
        push(&mut out, i..i + ch_len, t);
        i += ch_len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(code: &str, lang: Lang) -> Vec<(&str, Token)> {
        highlight(code, lang)
            .into_iter()
            .filter(|(_, t)| !matches!(t, Token::Plain | Token::Punct))
            .map(|(r, t)| (&code[r], t))
            .collect()
    }

    #[test]
    fn rust() {
        let code =
            "#[oxframe::scene]\nasync fn intro(v: View) { println!(\"hi\"); let x = 1.5; } // done";
        let t = tokens(code, Lang::Rust);
        assert!(t.contains(&("#[oxframe::scene]", Token::Attribute)));
        assert!(t.contains(&("async", Token::Keyword)));
        assert!(t.contains(&("intro", Token::Function)));
        assert!(t.contains(&("View", Token::Type)));
        assert!(t.contains(&("println!", Token::Macro)));
        assert!(t.contains(&("\"hi\"", Token::String)));
        assert!(t.contains(&("1.5", Token::Number)));
        assert!(t.contains(&("// done", Token::Comment)));
    }

    #[test]
    fn covers_everything_and_never_panics() {
        for (code, lang) in [
            ("const a = `x${y}`; // c", Lang::Js),
            ("def f(x):\n    return 'a' # c", Lang::Python),
            ("{\"a\": [1, true]}", Lang::Json),
            ("[dev]\nport = 3000", Lang::Toml),
            ("echo \"$HOME\" # c", Lang::Shell),
            ("'unterminated", Lang::Rust),
            ("\"open", Lang::Js),
            ("Привет мир", Lang::Rust),
            ("for i in 0..10 {}", Lang::Rust),
        ] {
            let h = highlight(code, lang);
            let joined: String = h.iter().map(|(r, _)| &code[r.clone()]).collect();
            assert_eq!(joined, code, "{lang:?}");
        }
        assert!(tokens("for i in 0..10 {}", Lang::Rust).contains(&("0", Token::Number)));
    }
}
