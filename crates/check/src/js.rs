//! JavaScript syntax: the story's JavaScript, `<<script>>` and `<script>` bodies, and the TwineScript expressions macros
//! take (`<<set $a to 1>>`, `<<if $a gte 3 and not $b>>`), parsed with oxc. A syntax error there only shows when a player
//! reaches it; here it shows before anyone plays.

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// A syntax error: where it is in the text that was checked, and oxc's words for it.
#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxError {
    pub offset: usize,
    pub message: String,
}

/// Parses a script: the story's JavaScript or a `<<script>>` body.
pub fn script(source: &str) -> Option<SyntaxError> {
    first_error(source, Some)
}

/// Parses a script written in TwineScript: `<<script TwineScript>>`.
pub fn script_twine(source: &str) -> Option<SyntaxError> {
    let (js, map) = desugar(source);
    first_error(&js, |offset| Some(map_back(&map, offset).min(source.len())))
}

/// Parses TwineScript statements, as `<<set>>` and `<<run>>` take them: `<<set $a to 1; $b to 2>>`.
pub fn statements(source: &str) -> Option<SyntaxError> {
    script_twine(source)
}

/// Parses a TwineScript expression, as `<<set>>`, `<<run>>`, `<<if>>` and `<<print>>` take one.
pub fn expression(source: &str) -> Option<SyntaxError> {
    let (js, map) = desugar(source);
    // Wrapped in parentheses, so `a = 1, b = 2` and an object literal both read as one expression.
    let wrapped = format!("(\n{js}\n);");
    first_error(&wrapped, |offset| {
        let inside = offset.checked_sub(2)?;
        Some(map_back(&map, inside.min(js.len())).min(source.len()))
    })
}

fn first_error(source: &str, place: impl Fn(usize) -> Option<usize>) -> Option<SyntaxError> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::script()).parse();
    let error = parsed.diagnostics.errors().next()?;
    let offset = error.labels.first().map(|l| l.offset() as usize).unwrap_or(0);
    Some(SyntaxError { offset: place(offset).unwrap_or(0), message: error.message.to_string() })
}

/// TwineScript's word operators as JavaScript: `to` is `=`, `is` is `===`, `gte` is `>=`, and the rest. Outside strings,
/// and not where the word is a property (`.to`) or a key (`{ to: 1 }`). Returns the JavaScript and, for mapping offsets
/// back, where each changed stretch starts in both.
pub fn desugar(source: &str) -> (String, Vec<(usize, usize)>) {
    const WORDS: &[(&str, &str)] = &[
        ("to", "="),
        ("is", "==="),
        ("isnot", "!=="),
        ("eq", "=="),
        ("neq", "!="),
        ("gt", ">"),
        ("gte", ">="),
        ("lt", "<"),
        ("lte", "<="),
        ("and", "&&"),
        ("or", "||"),
        ("not", "!"),
        ("def", "typeof "),
        ("ndef", "typeof "),
    ];
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    // (offset in the JavaScript, offset in the TwineScript) at the start of each stretch.
    let mut map = vec![(0usize, 0usize)];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b'"' | b'\'' | b'`') {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(bytes.len());
            out.push_str(&source[i..end]);
            i = end;
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'$') {
                j += 1;
            }
            let word = &source[i..j];
            let property = source[..i].trim_end().ends_with('.');
            let key = source[j..].trim_start().starts_with(':') && !source[j..].trim_start().starts_with("::");
            match WORDS.iter().find(|(w, _)| *w == word).filter(|_| !property && !key) {
                Some((_, js)) => {
                    out.push_str(js);
                    map.push((out.len(), j));
                }
                None => out.push_str(word),
            }
            i = j;
            continue;
        }
        let ch = source[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, map)
}

/// An offset in desugared JavaScript back to the TwineScript it came from, for the other checks.
pub fn map_back_public(map: &[(usize, usize)], offset: usize) -> usize {
    map_back(map, offset)
}

/// An offset in the JavaScript back to the TwineScript it came from.
fn map_back(map: &[(usize, usize)], offset: usize) -> usize {
    let (js, twine) = map.iter().rev().find(|(js, _)| *js <= offset).copied().unwrap_or((0, 0));
    twine + (offset - js)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_twinescript() {
        assert_eq!(desugar("$a to 1").0, "$a = 1");
        assert_eq!(desugar(r#"$a gte 3 and not $b is "to""#).0, r#"$a >= 3 && ! $b === "to""#);
        assert_eq!(desugar("$o.to is { to: 1 }").0, "$o.to === { to: 1 }");
        for good in ["$a to 1, $b to $a + 2", "$coins gte 3 and $bread isnot true", "setup.f($x, [1, 2], { k: 'v' })", "_i lt 3", "def $x", "$a to `t ${$b}`"] {
            assert_eq!(expression(good), None, "{good}");
        }
    }

    #[test]
    fn finds_errors_where_they_are() {
        let bad = expression("$a to (1 + 2").unwrap();
        assert!(bad.offset <= "$a to (1 + 2".len(), "{bad:?}");
        let bad = expression("$x gte 3 and and $y").unwrap();
        assert_eq!(&"$x gte 3 and and $y"[bad.offset..bad.offset + 3], "and", "{bad:?}");
        assert!(script("function f() { return 1 }").is_none());
        assert_eq!(statements("$a to 1; $b to 2"), None);
        assert!(expression("$random is 1 or if $x is 0").is_some(), "`or if` isn't TwineScript");
        let err = script("var a = {\n  b: 1,\n  c: 2,,\n};").unwrap();
        assert_eq!("var a = {\n  b: 1,\n  c: 2,,\n};"[..err.offset].matches('\n').count(), 2, "{err:?}");
    }
}
