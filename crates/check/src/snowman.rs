//! Snowman 2's code, as the checker reads it: the calls that go to, show or name a passage (its documented
//! `window.story` and global functions), and `<% … %>` templates left open.

/// What a call does with the passage it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Does {
    /// Goes there.
    Go,
    /// Shows it in place.
    Show,
    /// Only asks about it: it must exist, but it isn't a way there.
    Ask,
}

/// Which of a call's arguments name a passage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Which {
    First,
    Second,
    All,
}

/// `window.story` (also `story`) and its global functions, as Snowman 2's documentation gives them.
const CALLS: &[(&str, Which, Does)] = &[
    ("story.show(", Which::First, Does::Go),
    ("story.render(", Which::First, Does::Show),
    ("story.passage(", Which::First, Does::Ask),
    ("renderToSelector(", Which::Second, Does::Show),
    ("hasVisited(", Which::All, Does::Ask),
    ("visited(", Which::All, Does::Ask),
];

/// A passage named in a call: where the call is, the passage as written (a string literal's value, or the expression),
/// whether it's a literal, and what the call does with it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Named {
    pub(crate) offset: usize,
    pub(crate) passage: String,
    pub(crate) literal: bool,
    pub(crate) does: Does,
}

/// Every passage a piece of code names through Snowman's calls.
pub(crate) fn named(code: &str) -> Vec<Named> {
    let mut out = vec![];
    for &(needle, which, does) in CALLS {
        for (at, _) in code.match_indices(needle) {
            // Not part of a longer name: `hasVisited(` isn't `visited(`; `window.story.show(` is `story.show(`.
            let before = code[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                continue;
            }
            let open = at + needle.len() - 1;
            let Some(close) = closing_paren(code.as_bytes(), open) else { continue };
            let args = crate::harlowe::args(&code[open + 1..close]);
            let picked: Vec<&String> = match which {
                Which::First => args.first().into_iter().collect(),
                Which::Second => args.get(1).into_iter().collect(),
                Which::All => args.iter().collect(),
            };
            for arg in picked {
                match crate::harlowe::literal(arg) {
                    Some(passage) => out.push(Named { offset: at, passage, literal: true, does }),
                    None => out.push(Named { offset: at, passage: arg.clone(), literal: false, does }),
                }
            }
        }
    }
    out.sort_by_key(|n| n.offset);
    out
}

/// Whether code moves the player on: a call that goes to a passage, whatever it names.
pub(crate) fn goes_on(code: &str) -> bool {
    named(code).iter().any(|n| n.does == Does::Go)
}

fn closing_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut at = open;
    while at < bytes.len() {
        match bytes[at] {
            b'"' | b'\'' | b'`' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
            }
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

/// `[[…]]` links, read as Snowman 2's own pattern reads them: words, then at most one `|`, `->` or `<-` and a passage;
/// no part holds `]`, `|` or `<`, nor a `-` before `>`; and nothing is trimmed, so `[[Go -> Hall]]` goes to " Hall".
/// Anything else between `[[` and `]]` isn't a link to Snowman, and shows as written.
pub(crate) fn links(markup: &str) -> Vec<crate::scan::Link> {
    // One part: at least one character, none of `]`, `|`, `<`, and no `-` that starts `->`.
    fn part(s: &str) -> bool {
        !s.is_empty() && !s.contains([']', '|', '<']) && !s.contains("->")
    }
    let mut out = vec![];
    let mut at = 0;
    while let Some(start) = markup[at..].find("[[").map(|i| at + i) {
        let Some(close) = markup[start + 2..].find("]]").map(|i| start + 2 + i) else { break };
        let inner = &markup[start + 2..close];
        at = start + 2;
        let split = [("|", inner.find('|')), ("->", inner.find("->")), ("<-", inner.find("<-"))]
            .into_iter()
            .filter_map(|(sep, i)| i.map(|i| (sep, i)))
            .min_by_key(|(_, i)| *i);
        let (text, target) = match split {
            Some(("<-", i)) => (&inner[i + 2..], &inner[..i]),
            Some((sep, i)) => (&inner[..i], &inner[i + sep.len()..]),
            None => (inner, inner),
        };
        let whole = if split.is_some() { part(text) && part(target) } else { part(inner) };
        if whole {
            out.push(crate::scan::Link { start, end: close + 2, text: text.to_string(), target: target.to_string(), setter: None });
            at = close + 2;
        }
    }
    out
}

/// Where each `<%` with no `%>` after it starts.
pub(crate) fn unclosed_templates(text: &str) -> Vec<usize> {
    let mut out = vec![];
    let mut at = 0;
    while let Some(open) = text[at..].find("<%").map(|i| at + i) {
        match text[open + 2..].find("%>") {
            Some(close) => at = open + 2 + close + 2,
            None => {
                out.push(open);
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_that_name_passages_in_each_version() {
        let code = "<% if (hasVisited('Hall', 'Den')) { story.show('Garden') } %> <%= story.render(s.next) %> <% renderToSelector('#ui', 'Menu') %>";
        let all = named(code);
        let found: Vec<(&str, bool, Does)> = all.iter().map(|n| (n.passage.as_str(), n.literal, n.does)).collect();
        assert_eq!(
            found,
            [("Hall", true, Does::Ask), ("Den", true, Does::Ask), ("Garden", true, Does::Go), ("s.next", false, Does::Show), ("Menu", true, Does::Show)]
        );
        assert!(goes_on("window.story.show(\"Hall\")"));
    }

    #[test]
    fn links_as_snowman_reads_them() {
        let targets = |m: &str| links(m).into_iter().map(|l| l.target).collect::<Vec<_>>();
        assert_eq!(targets("[[Hall]] [[Go -> Hall]] [[Den<-Back]] [[a|b]]"), ["Hall", " Hall", "Den", "b"]);
        assert!(targets("[[a|b|c]] [[a->b->c]] [[<b>Bold</b>]]").is_empty(), "not links to Snowman");
    }

    #[test]
    fn templates_left_open() {
        assert!(unclosed_templates("<% s.x = 1 %> <%= s.x %>").is_empty());
        assert_eq!(unclosed_templates("Hi <% s.x = 1 %> and <%= s.x "), [21]);
    }
}
