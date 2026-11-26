//! Harlowe's markup, as the checker reads it: macro calls `(name: args)`, hooks `[…]` and links `[[…]]`; and its macros,
//! from the table generated out of the bundled format (`table.rs`).

pub(crate) mod table;

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::scan::Link;

/// One Harlowe macro, as its format documents it.
#[derive(Debug)]
// The signature and what it gives are kept for checks of arguments to come.
#[allow(dead_code)]
pub(crate) struct MacroDef {
    pub(crate) name: &'static str,
    pub(crate) aka: &'static [&'static str],
    pub(crate) sig: &'static str,
    /// `Changer`, `Command`, `Instant`, or the type of value it gives.
    pub(crate) returns: &'static str,
    pub(crate) category: &'static str,
}

/// A macro name as Harlowe matches it: "case-insensitive, dash-insensitive and underscore-insensitive".
pub(crate) fn normalise(name: &str) -> String {
    name.chars().filter(|c| *c != '-' && *c != '_').flat_map(char::to_lowercase).collect()
}

/// The macro a name calls, under any of its names, however it's written: `(goto:)`, `(Go_To:)`.
pub(crate) fn lookup(name: &str) -> Option<&'static MacroDef> {
    static BY_NAME: OnceLock<HashMap<String, &'static MacroDef>> = OnceLock::new();
    let by_name = BY_NAME.get_or_init(|| {
        let mut map = HashMap::new();
        for def in table::MACROS {
            for name in std::iter::once(&def.name).chain(def.aka) {
                map.insert(normalise(name), def);
            }
        }
        map
    });
    by_name.get(&normalise(name)).copied()
}

/// Every name and alias, for "did you mean".
pub(crate) fn all_names() -> Vec<String> {
    table::MACROS.iter().flat_map(|d| std::iter::once(d.name).chain(d.aka.iter().copied())).map(str::to_string).collect()
}

/// Macros a story adds through its JavaScript, as add-ons such as Chapel's Harlowe Audio Library do:
/// `Chapel.Macros.add({ newtrack: function () { … }, … })`, each key a macro's name, normalised.
pub(crate) fn script_macros<'a>(code: impl IntoIterator<Item = &'a str>) -> std::collections::HashSet<String> {
    let key = regex::Regex::new(r"([A-Za-z_$][\w$-]*|'[^']+'|\x22[^\x22]+\x22)\s*:\s*function\b").unwrap();
    let mut out = std::collections::HashSet::new();
    for text in code {
        for (at, _) in text.match_indices("Macros.add(") {
            let open = at + "Macros.add(".len() - 1;
            let end = closing(text.as_bytes(), open).unwrap_or(text.len());
            for found in key.captures_iter(&text[open..end]) {
                out.insert(normalise(found[1].trim_matches(['\'', '"'])));
            }
        }
    }
    out
}

/// The `)` closing the `(` at `open`, past strings.
fn closing(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut at = open;
    while at < bytes.len() {
        match bytes[at] {
            b'"' | b'\'' | b'`' => at = string_end(bytes, at) - 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
        at += 1;
    }
    None
}

/// A macro call, `(name: args)`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Call {
    /// Where its `(` is.
    pub(crate) start: usize,
    /// Just after its `)`, or the end of the text when it's never closed.
    pub(crate) end: usize,
    /// The name as written, without the `:`; `$name` or `_name` for a custom macro kept in a variable.
    pub(crate) name: String,
    pub(crate) args: String,
    pub(crate) args_start: usize,
    pub(crate) closed: bool,
}

impl Call {
    /// A custom macro, called through the variable that holds it: `($greet:)`.
    pub(crate) fn is_custom(&self) -> bool {
        self.name.starts_with(['$', '_'])
    }
}

/// Every macro call in a passage's markup, outer ones before those in their arguments. Verbatim text is no code: a
/// run of backticks to the next run as long, and the hook a `(verbatim:)` (or `(v6m:)`) changer is attached to.
pub(crate) fn calls(markup: &str) -> Vec<Call> {
    scan_markup(markup).0
}

/// The stretches of a passage that are verbatim, shown as written: no link, hook or call in them is one.
pub(crate) fn verbatim(markup: &str) -> Vec<(usize, usize)> {
    scan_markup(markup).1
}

/// One pass over a passage: its calls, and its verbatim stretches (a run of backticks to the next run as long, and the
/// hook a `(verbatim:)` changer is attached to).
fn scan_markup(markup: &str) -> (Vec<Call>, Vec<(usize, usize)>) {
    let bytes = markup.as_bytes();
    let (mut out, mut spans) = (vec![], vec![]);
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let run = bytes[at..].iter().take_while(|b| **b == b'`').count();
            let fence = "`".repeat(run);
            let end = markup[at + run..].find(&fence).map(|i| at + run + i + run).unwrap_or(bytes.len());
            spans.push((at, end));
            at = end;
        } else if bytes[at] == b'(' && call_name(markup, at).is_some() {
            let first = out.len();
            at = call(markup, at, &mut out);
            if matches!(normalise(&out[first].name).as_str(), "verbatim" | "v6m") && out[first].closed {
                let rest = &markup[at..];
                let gap = rest.len() - rest.trim_start().len();
                if rest[gap..].starts_with('[') {
                    let end = hook_end(bytes, at + gap);
                    spans.push((at + gap, end));
                    at = end;
                }
            }
        } else {
            at += 1;
        }
    }
    (out, spans)
}

/// Past the hook whose `[` is at `open`: its matching `]`, or the end.
fn hook_end(bytes: &[u8], open: usize) -> usize {
    let mut depth = 0;
    for (i, b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    bytes.len()
}

/// The name of a call starting at `(`: letters, digits, `-` and `_` (or a variable), then `:`.
fn call_name(markup: &str, open: usize) -> Option<&str> {
    let rest = &markup[open + 1..];
    let lead = if rest.starts_with(['$', '_']) { 1 } else { 0 };
    let len = rest[lead..].find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_')).unwrap_or(rest.len() - lead);
    let name = &rest[..lead + len];
    // `name:` then anything but `/`: `(http://…` is words, not a call.
    let colon = rest[lead + len..].starts_with(':') && !rest[lead + len + 1..].starts_with('/');
    let valid = len > 0 && colon && (lead == 1 || !name.starts_with('_'));
    valid.then_some(name)
}

/// Reads the call at `open`, and the calls in its arguments, into `out`; returns where it ends.
fn call(markup: &str, open: usize, out: &mut Vec<Call>) -> usize {
    let bytes = markup.as_bytes();
    let name = call_name(markup, open).unwrap_or_default().to_string();
    let args_start = open + 1 + name.len() + 1;
    let index = out.len();
    out.push(Call { start: open, end: markup.len(), name, args: String::new(), args_start, closed: false });
    let mut at = args_start;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' if possessive(bytes, at) => at += 2,
            b'"' | b'\'' => at = string_end(bytes, at),
            b'(' if call_name(markup, at).is_some() => at = call(markup, at, out),
            b'(' => at = group_end(markup, at, out),
            b')' => {
                let call = &mut out[index];
                call.args = markup[args_start..at].to_string();
                call.end = at + 1;
                call.closed = true;
                return at + 1;
            }
            _ => at += 1,
        }
    }
    out[index].args = markup[args_start.min(markup.len())..].to_string();
    markup.len()
}

/// Past a plain `( … )` group in an expression, with any calls in it read.
fn group_end(markup: &str, open: usize, out: &mut Vec<Call>) -> usize {
    let bytes = markup.as_bytes();
    let mut at = open + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\'' if possessive(bytes, at) => at += 2,
            b'"' | b'\'' => at = string_end(bytes, at),
            b'(' if call_name(markup, at).is_some() => at = call(markup, at, out),
            b'(' => at = group_end(markup, at, out),
            b')' => return at + 1,
            _ => at += 1,
        }
    }
    markup.len()
}

/// Whether the `'` at `at` is Harlowe's possessive, `(passage:)'s name`, `$x's 1st`, rather than a string's quote: right
/// after a name, `)` or `]`, and then `s` and something that isn't part of a word.
fn possessive(bytes: &[u8], at: usize) -> bool {
    at > 0
        && (bytes[at - 1].is_ascii_alphanumeric() || matches!(bytes[at - 1], b'_' | b')' | b']'))
        && bytes.get(at + 1) == Some(&b's')
        && !bytes.get(at + 2).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// Past a quoted string starting at `at` (its quote), with `\` escapes.
fn string_end(bytes: &[u8], at: usize) -> usize {
    let quote = bytes[at];
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// A call's arguments, split at the commas between them (not those in strings, calls or brackets), trimmed.
pub(crate) fn args(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let (mut out, mut depth, mut from, mut at) = (vec![], 0i32, 0, 0);
    while at < bytes.len() {
        match bytes[at] {
            b'\'' if possessive(bytes, at) => {
                at += 2;
                continue;
            }
            b'"' | b'\'' => {
                at = string_end(bytes, at);
                continue;
            }
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b',' if depth == 0 => {
                out.push(text[from..at].trim().to_string());
                from = at + 1;
            }
            _ => {}
        }
        at += 1;
    }
    let last = text[from..].trim();
    if !last.is_empty() || !out.is_empty() {
        out.push(last.to_string());
    }
    out
}

/// An argument that is one string literal and nothing else: its value. `"Hall"` is; `"Day" + $n` isn't.
pub(crate) fn literal(arg: &str) -> Option<String> {
    let arg = arg.trim();
    let quote = *arg.as_bytes().first()?;
    if !matches!(quote, b'"' | b'\'') || string_end(arg.as_bytes(), 0) != arg.len() || arg.len() < 2 {
        return None;
    }
    Some(arg[1..arg.len() - 1].replace(&format!("\\{}", quote as char), &(quote as char).to_string()))
}

/// Which argument of a macro names a passage to go to or show, given how many it has: `(go-to: "Hall")`, the first;
/// `(link-goto: "Go", "Hall")`, the second, or with one, the first, which is both the words and the passage.
pub(crate) fn passage_arg(name: &str, count: usize) -> Option<usize> {
    let def = lookup(name)?;
    match def.name {
        "go-to" | "redirect" | "display" => Some(0),
        "link-goto" | "link-reveal-goto" => Some(if count >= 2 { 1 } else { 0 }),
        "click-goto" | "mouseover-goto" | "mouseout-goto" => Some(1),
        _ => None,
    }
}

/// Macros that move the player on: a passage that runs one isn't a dead end.
pub(crate) fn navigates(name: &str) -> bool {
    lookup(name).is_some_and(|def| {
        matches!(
            def.name,
            "go-to"
                | "redirect"
                | "link-goto"
                | "link-reveal-goto"
                | "click-goto"
                | "mouseover-goto"
                | "mouseout-goto"
                | "undo"
                | "link-undo"
                | "click-undo"
                | "mouseover-undo"
                | "mouseout-undo"
                | "restart"
                | "icon-undo"
                | "icon-restart"
                | "load-game"
        )
    })
}

/// `[[…]]` links, read as Harlowe's lexer reads them: the last `->` or `|` divides the words from the passage, else the
/// first `<-` does; `[[[Go]]]` is a hook holding a link, since a link can't open with three brackets.
pub(crate) fn links(markup: &str) -> Vec<Link> {
    let verbatim = verbatim(markup);
    let mut out = vec![];
    let mut at = 0;
    while let Some(found) = markup[at..].find("[[") {
        let mut start = at + found;
        // In a run of brackets the link takes the last two: the ones before are hooks.
        while markup[start + 2..].starts_with('[') {
            start += 1;
        }
        let inner_start = start + 2;
        let Some(close) = markup[inner_start..].find("]]") else { break };
        let inner = &markup[inner_start..inner_start + close];
        let end = inner_start + close + 2;
        let shown_as_written = verbatim.iter().any(|&(a, b)| start >= a && start < b);
        if !inner.contains('\n') && !inner.trim().is_empty() && !shown_as_written {
            let (text, target) = split_link(inner);
            out.push(Link { start, end, text, target, setter: None });
        }
        at = end;
    }
    out
}

/// A link's words and its passage, as Harlowe divides them. Nothing is trimmed: Harlowe makes the link a
/// `(link-goto:)` with the passage exactly as written, so `[[Go -> Hall]]` goes to " Hall".
fn split_link(inner: &str) -> (String, String) {
    let right = [inner.rfind("->").map(|at| (at, 2)), inner.rfind('|').map(|at| (at, 1))].into_iter().flatten().max_by_key(|(at, _)| *at);
    if let Some((at, len)) = right {
        return (inner[..at].to_string(), inner[at + len..].to_string());
    }
    if let Some(at) = inner.find("<-") {
        return (inner[at + 2..].to_string(), inner[..at].to_string());
    }
    (inner.to_string(), inner.to_string())
}

/// Hooks opened right after a call or a hook name (`(if: …)[`, `|name>[`) that are never closed: Harlowe runs them to
/// the end of the passage, which is seldom what was meant. `[=` asks for exactly that, so it isn't one. Gives each
/// unclosed `[`'s offset.
pub(crate) fn unclosed_hooks(markup: &str, calls: &[Call]) -> Vec<usize> {
    // Inside a call's arguments, brackets are the expression's own.
    let verbatim = verbatim(markup);
    let inside = |at: usize| calls.iter().any(|c| at >= c.args_start && at < c.end.saturating_sub(1)) || verbatim.iter().any(|&(a, b)| at >= a && at < b);
    let bytes = markup.as_bytes();
    let mut open: Vec<(usize, bool)> = vec![];
    let mut out = vec![];
    for at in 0..bytes.len() {
        if inside(at) {
            continue;
        }
        match bytes[at] {
            b'[' => {
                // Attached across whitespace and line breaks, as Harlowe finds the element after a changer.
                let before = markup[..at].trim_end();
                let attached = calls.iter().any(|c| c.closed && c.end == before.len()) || before.ends_with('>') && before.contains('|');
                let open_form = markup[at + 1..].starts_with('=');
                open.push((at, attached && !open_form));
            }
            b']' => {
                open.pop();
            }
            _ => {}
        }
    }
    out.extend(open.into_iter().filter(|(_, report)| *report).map(|(at, _)| at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table is the bundled format's own: regenerate it (scripts/harlowe-macros.py) when the format changes.
    #[test]
    fn the_table_matches_the_bundled_harlowe() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../build/formats/harlowe-3/format.js");
        let source = std::fs::read_to_string(path).unwrap();
        let version = regex::Regex::new(r#""version"\s*:\s*"([^"]+)""#).unwrap().captures(&source).unwrap()[1].to_string();
        let names: std::collections::BTreeSet<String> =
            regex::Regex::new(r#"name:"([^"]+)",sig:""#).unwrap().captures_iter(&source).map(|c| c[1].to_string()).collect();
        let table: std::collections::BTreeSet<String> = table::MACROS.iter().map(|d| d.name.to_string()).collect();
        assert_eq!(version, table::VERSION, "run scripts/harlowe-macros.py");
        assert_eq!(names, table, "run scripts/harlowe-macros.py");
    }

    #[test]
    fn names_match_as_harlowe_matches_them() {
        assert_eq!(lookup("goto").unwrap().name, "go-to");
        assert_eq!(lookup("Go_To").unwrap().name, "go-to");
        assert_eq!(lookup("link-replace").unwrap().name, "link", "an alias");
        assert!(lookup("lnk-goto").is_none());
    }

    #[test]
    fn calls_with_strings_and_calls_inside() {
        let found = calls("Hi (if: $a is \"x)\")[(print: (either: 1, 2))] (note: never closed");
        let names: Vec<(&str, bool)> = found.iter().map(|c| (c.name.as_str(), c.closed)).collect();
        assert_eq!(names, [("if", true), ("print", true), ("either", true), ("note", false)]);
        assert_eq!(found[0].args.trim(), "$a is \"x)\"");
        assert!(calls("It's (just) words: (and) more.").is_empty());
        assert_eq!(calls("($greet: \"Ava\")")[0].name, "$greet");
        let possessive = calls("(set: $n to (passage:)'s name)(print: $list's 1st)(if: $a's b is 'x')[y]");
        assert!(possessive.iter().all(|c| c.closed), "{possessive:?}");
        assert!(calls("See (http://example.com) and `(not: a call)` and ``(nor: `this`)``").is_empty());
        let verbatim = calls("(verbatim:)[(nope:)] (v6m:) [(nor:)] (print: 1)");
        assert_eq!(verbatim.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["verbatim", "v6m", "print"]);
    }

    #[test]
    fn arguments_and_literals() {
        assert_eq!(args(" \"Go, now\", \"Hall\" "), ["\"Go, now\"", "\"Hall\""]);
        assert_eq!(literal("\"Hall\""), Some("Hall".into()));
        assert_eq!(literal("\"Day\" + $n"), None);
        assert_eq!(passage_arg("link-goto", 1), Some(0));
        assert_eq!(passage_arg("linkgoto", 2), Some(1));
    }

    #[test]
    fn links_as_harlowe_divides_them() {
        let targets = |m: &str| links(m).into_iter().map(|l| l.target).collect::<Vec<_>>();
        assert_eq!(targets("[[A->B->C]] [[D<-E<-F]] [[Plain]]"), ["C", "D", "Plain"]);
        assert_eq!(targets("(if: $x)[[[Go]]]"), ["Go"]);
        assert_eq!(targets("[[Enter|Hall]] [[a|b->c]] [[a->b|c]]"), ["Hall", "c", "c"], "the last -> or | divides");
        assert_eq!(targets("[[Go -> Hall]]"), [" Hall"], "taken exactly as written");
        assert!(targets("``[[Shown as written]]`` (verbatim:)[ [[Nor this]] ] `[ an open bracket`").is_empty(), "verbatim isn't a link");
    }

    #[test]
    fn hooks_left_open() {
        let m = "(if: $a)[Yes (if: $b)\n  [Inner] |x>[named and open\n[plain text, not a hook";
        let c = calls(m);
        let open = unclosed_hooks(m, &c);
        assert_eq!(open.len(), 2, "{open:?}");
        assert!(unclosed_hooks("(if: $a)[=runs to the end", &calls("(if: $a)[=runs to the end")).is_empty());
    }
}
