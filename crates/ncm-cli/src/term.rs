//! Make text from outside the program safe to print on a terminal.
//!
//! Terminals act on control sequences in their output: `ESC [ … m` recolors
//! and moves the cursor, OSC 52 writes the clipboard, and so on. Song titles
//! come from the NCM file and file names from whoever named the files, so
//! printing either as-is would let a crafted file spoof what's on screen or
//! drive the terminal. Anything we print from those sources goes through
//! [`printable`] first.

use std::borrow::Cow;
use std::path::Path;

/// `text` with every control character (C0 such as ESC, BEL, CR, LF and
/// tab; DEL; C1 such as CSI and OSC) and every bidirectional embedding,
/// override or isolate character written as a `\u{…}` escape. Everything
/// else, CJK and emoji included, passes through unchanged.
pub(crate) fn printable(text: &str) -> Cow<'_, str> {
    if !text.chars().any(needs_escape) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if needs_escape(c) {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// [`printable`] for a path, as `Path::display` would show it.
pub(crate) fn printable_path(path: &Path) -> String {
    printable(&path.to_string_lossy()).into_owned()
}

fn needs_escape(c: char) -> bool {
    // Bidi controls can't drive the terminal, but they can reorder how the
    // rest of the line reads.
    c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_text_passes_through_unchanged() {
        let text = "周杰伦 - 晴天 (Live) · Mr. Brightside 🎵";
        assert!(matches!(printable(text), Cow::Borrowed(t) if t == text));
    }

    #[test]
    fn escape_sequences_are_shown_not_sent() {
        // OSC 52 (set clipboard), then CSI 2J (clear screen).
        assert_eq!(
            printable("a\u{1b}]52;c;ZXZpbA==\u{7}b\u{1b}[2J"),
            "a\\u{1b}]52;c;ZXZpbA==\\u{7}b\\u{1b}[2J"
        );
    }

    #[test]
    fn c1_controls_and_line_breaks_are_escaped() {
        // U+009B is a one-character CSI; a newline could fake another row.
        assert_eq!(
            printable("a\u{9b}2Jb\r\nTitle:\tc\u{7f}"),
            "a\\u{9b}2Jb\\u{d}\\u{a}Title:\\u{9}c\\u{7f}"
        );
    }

    #[test]
    fn bidi_overrides_are_escaped() {
        assert_eq!(printable("abc\u{202e}fed"), "abc\\u{202e}fed");
        assert_eq!(printable("x\u{2066}y\u{2069}"), "x\\u{2066}y\\u{2069}");
    }

    #[test]
    fn paths_are_escaped_too() {
        assert_eq!(
            printable_path(Path::new("dir/a\u{1b}[31mb.ncm")),
            "dir/a\\u{1b}[31mb.ncm"
        );
    }
}
