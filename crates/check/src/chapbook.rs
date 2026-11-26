//! Chapbook's markup, as the checker reads it: a passage's vars section, its modifiers (`[if …]` lines), and its inserts
//! (`{link to: 'Hall'}`, `{name}`). The inserts and modifiers it knows are the bundled Chapbook's own, from the patterns
//! in its format.js (a test holds the two together).

use regex::Regex;
use std::sync::OnceLock;

/// The bundled Chapbook's version, which these patterns come from.
pub(crate) const VERSION: &str = "2.3.1";

/// Every insert's pattern, as Chapbook matches an insert's name: case-insensitive, from the start.
pub(crate) const INSERTS: &[&str] = &[
    r"^(no )?ambient\s+sound",
    r"^back\s+link",
    r"^cycling\s+link(\s+for)?",
    r"^dropdown menu(\s+for)?",
    r"^embed\s+flickr(\s+image)?",
    r"^embed\s+image",
    r"^embed\s+passage(\s+named)?",
    r"^embed\s+youtube(\s+video)?",
    r"^link\s+to",
    r"^restart\s+link",
    r"^reveal\s+link",
    r"^sound\s+effect",
    r"^text\s+input(\s+for)?",
    r"^theme\s+switcher$",
];

/// Every modifier's pattern.
pub(crate) const MODIFIERS: &[&str] = &[
    r"^after\s",
    r"^align\s+(left|right|center)",
    r"^append$",
    r"^if(always|never)?\s|else$|unless\s",
    r"^continued?|cont('d)?$",
    r"^css$",
    r"^javascript$",
    r"^(note(\s+to\s+myself)?|n\.?b\.?|todo|fixme)$",
];

/// Names Chapbook keeps for itself, never a story's variable.
pub(crate) const BUILT_IN: &[&str] = &["browser", "config", "engine", "now", "passage", "random", "story"];

fn compiled(patterns: &'static [&'static str], cell: &'static OnceLock<Vec<Regex>>) -> &'static [Regex] {
    cell.get_or_init(|| patterns.iter().map(|p| Regex::new(&format!("(?i){p}")).unwrap()).collect())
}

pub(crate) fn is_insert(name: &str) -> bool {
    static CELL: OnceLock<Vec<Regex>> = OnceLock::new();
    compiled(INSERTS, &CELL).iter().any(|r| r.is_match(name.trim()))
}

pub(crate) fn is_modifier(text: &str) -> bool {
    static CELL: OnceLock<Vec<Regex>> = OnceLock::new();
    compiled(MODIFIERS, &CELL).iter().any(|r| r.is_match(text.trim()))
}

/// The inserts and modifiers a story adds itself, from its JavaScript: each `match: /…/` in an
/// `engine.template.inserts.add({…})` (or `modifiers`), or in an array given to `inserts` or `modifiers`.
#[derive(Debug, Default)]
pub(crate) struct Custom {
    pub(crate) inserts: Vec<Regex>,
    pub(crate) modifiers: Vec<Regex>,
}

impl Custom {
    pub(crate) fn of<'a>(code: impl IntoIterator<Item = &'a str>) -> Custom {
        static MATCH: OnceLock<Regex> = OnceLock::new();
        let match_re = MATCH.get_or_init(|| Regex::new(r"match\s*:\s*/((?:[^/\\\n]|\\.)+)/([a-z]*)").unwrap());
        let mut out = Custom::default();
        for text in code {
            for found in match_re.captures_iter(text) {
                let at = found.get(0).unwrap().start();
                // Which list it goes in: whichever of the two words comes last before it.
                let head = &text[..at];
                let (inserts, modifiers) = (head.rfind("inserts"), head.rfind("modifiers"));
                let flags = if found[2].contains('i') { "(?i)" } else { "" };
                let Ok(pattern) = Regex::new(&format!("{flags}{}", &found[1])) else { continue };
                match (inserts, modifiers) {
                    (Some(i), Some(m)) if m > i => out.modifiers.push(pattern),
                    (_, Some(_)) if inserts.is_none() => out.modifiers.push(pattern),
                    (Some(_), _) => out.inserts.push(pattern),
                    _ => {}
                }
            }
        }
        out
    }
}

/// A passage, read as Chapbook reads it.
#[derive(Debug, Default)]
pub(crate) struct Passage {
    /// Each variable the vars section sets: its name (to the first `.`), and where.
    pub(crate) sets: Vec<(String, usize)>,
    /// Each modifier line: where it starts, and each modifier in it (`[if x; append]` is two).
    pub(crate) modifiers: Vec<(usize, Vec<String>)>,
    /// Each `{…}` in text that shows: an insert or a variable.
    pub(crate) inserts: Vec<Insert>,
    /// What the vars section puts in the header or footer (`config.footer.center: "[[Contents]]"`): where, and the text,
    /// which every passage then shows.
    pub(crate) frame: Vec<(usize, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Insert {
    pub(crate) start: usize,
    /// The insert's name as written, `link to`; or, for `{name}`, the variable.
    pub(crate) name: String,
    /// The value after the name, `'Hall'` in `{link to: 'Hall'}`, as written.
    pub(crate) value: Option<String>,
    /// The properties after it: `passage: 'Hall'` as ("passage", "'Hall'").
    pub(crate) props: Vec<(String, String)>,
    /// `{name}` or `{name.part}`: a variable shown, not an insert.
    pub(crate) variable: bool,
}

impl Insert {
    /// The passage it goes to or shows, as written, and whether it shows it in place (rather than going there).
    pub(crate) fn passage(&self) -> Option<(&str, bool)> {
        let name = self.name.to_ascii_lowercase();
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
        match name.as_str() {
            "link to" => self.value.as_deref().map(|v| (v, false)),
            "embed passage" | "embed passage named" => self.value.as_deref().map(|v| (v, true)),
            "reveal link" => self.props.iter().find(|(k, _)| k == "passage").map(|(_, v)| (v.as_str(), true)),
            _ => None,
        }
    }
}

/// Reads a passage's vars section, modifiers and inserts. The text under `[JavaScript]`, `[CSS]` and note modifiers
/// is code or notes, not text: its braces are no inserts.
pub(crate) fn read(text: &str) -> Passage {
    let mut out = Passage::default();
    let body_start = vars_section(text, &mut out.sets, &mut out.frame);
    let mut hidden = false;
    let mut at = body_start;
    for line in text[body_start..].split_inclusive('\n') {
        // Chapbook's own rule: the whole line `[…]`, at least three characters inside, the first not `[`, the last not `]`.
        let whole = line.trim_end_matches(['\n', '\r']);
        if let Some(inner) =
            whole.strip_prefix('[').and_then(|t| t.strip_suffix(']')).filter(|i| i.chars().count() >= 3 && !i.starts_with('[') && !i.ends_with(']'))
        {
            let mods: Vec<String> = inner.split(';').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect();
            hidden = mods.iter().any(|m| {
                let m = m.to_ascii_lowercase();
                m == "javascript"
                    || m == "css"
                    || (is_modifier(&m) && (m.starts_with("note") || m.starts_with("n.") || m == "nb" || m == "todo" || m == "fixme"))
            });
            out.modifiers.push((at + line.find('[').unwrap_or(0), mods));
        } else if !hidden {
            inserts(line, at, &mut out.inserts);
        }
        at += line.len();
    }
    out
}

/// The vars section, read as Chapbook's own code reads it: everything before the first line that is exactly `--`.
/// Each line with a colon sets the name before it (less a `(condition)`); a line without one, such as a `//` comment,
/// Chapbook ignores. Its variables go into `sets`, the header and footer it sets into `frame`; gives where the text
/// after it starts.
fn vars_section(text: &str, sets: &mut Vec<(String, usize)>, frame: &mut Vec<(usize, String)>) -> usize {
    let mut at = 0;
    let mut lines = vec![];
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "--" {
            for (start, line) in lines {
                let line: &str = line;
                let Some(colon) = line.find(':') else { continue };
                let written = line[..colon].trim();
                let name = written.find('(').map_or(written, |i| written[..i].trim());
                if (name.starts_with("config.header.") || name.starts_with("config.footer."))
                    && let Some(value) = literal(line[colon + 1..].trim())
                {
                    frame.push((start, value));
                }
                if !name.is_empty() {
                    sets.push((name.split('.').next().unwrap_or(name).to_string(), start));
                }
            }
            return at + line.len();
        }
        lines.push((at, line));
        at += line.len();
    }
    0
}

/// The `{…}` in a line of text.
pub(crate) fn inserts(line: &str, offset: usize, out: &mut Vec<Insert>) {
    let bytes = line.as_bytes();
    let mut at = 0;
    while let Some(open) = line[at..].find('{').map(|i| at + i) {
        let Some(close) = closing_brace(bytes, open) else { break };
        let inner = line[open + 1..close].trim();
        at = close + 1;
        if inner.is_empty() {
            continue;
        }
        // Its name runs to the first `:` or `,`: `{link to: 'Hall'}`, `{back link, label: 'Back'}`, `{restart link}`.
        let cut = inner.find([':', ',']).unwrap_or(inner.len());
        let name = inner[..cut].trim().to_string();
        let rest = inner[cut..].trim_start_matches([':', ',']).trim();
        let simple = |s: &str| {
            s.split('.').all(|p| {
                p.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$') && p.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')
            })
        };
        if cut == inner.len() && simple(&name) {
            out.push(Insert { start: offset + open, name, value: None, props: vec![], variable: true });
            continue;
        }
        let mut parts = crate::harlowe::args(rest).into_iter();
        let value = if inner[cut..].starts_with(':') { parts.next() } else { None };
        let props = parts.filter_map(|p| p.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
        out.push(Insert { start: offset + open, name, value, props, variable: false });
    }
}

/// The `}` that closes the `{` at `open`, past strings and brackets inside.
fn closing_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut at = open;
    while at < bytes.len() {
        match bytes[at] {
            b'"' | b'\'' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() && bytes[at] != quote {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
            }
            b'{' => depth += 1,
            b'}' => {
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

/// `[[…]]` links, read as Chapbook's own code reads them: the first `|`, else the first `->`, else the first `<-`
/// divides the words from the passage, and nothing is trimmed: `[[Wait ]]` goes to a passage named "Wait ", exactly.
pub(crate) fn links(markup: &str) -> Vec<crate::scan::Link> {
    let mut out = vec![];
    let mut at = 0;
    while let Some(start) = markup[at..].find("[[").map(|i| at + i) {
        let Some(close) = markup[start + 2..].find("]]").map(|i| start + 2 + i) else { break };
        let inner = &markup[start + 2..close];
        at = close + 2;
        if inner.contains('\n') {
            continue;
        }
        let (text, target) = if let Some(i) = inner.find('|') {
            (&inner[..i], &inner[i + 1..])
        } else if let Some(i) = inner.find("->") {
            (&inner[..i], &inner[i + 2..])
        } else if let Some(i) = inner.find("<-") {
            (&inner[i + 2..], &inner[..i])
        } else {
            (inner, inner)
        };
        // Chapbook leaves a link with no words or no passage as it was written.
        if text.is_empty() || target.is_empty() {
            continue;
        }
        out.push(crate::scan::Link { start, end: close + 2, text: text.to_string(), target: target.to_string(), setter: None });
    }
    out
}

/// A value as written in an insert: `'Hall'` or `"Hall"` is that passage; anything else is worked out as it runs.
pub(crate) fn literal(value: &str) -> Option<String> {
    crate::harlowe::literal(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patterns are the bundled Chapbook's own: update them with the format.
    #[test]
    fn the_patterns_match_the_bundled_chapbook() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../build/formats/chapbook-2/format.js")).unwrap();
        assert!(source.contains(&format!("\"version\":\"{VERSION}\"")), "the bundled Chapbook isn't {VERSION}");
        let found: Vec<String> = Regex::new(r#"match:\s*/((?:[^/\\]|\\.)+)/i"#).unwrap().captures_iter(&source).map(|c| c[1].replace("\\\\", "\\")).collect();
        let ours: Vec<String> = INSERTS.iter().chain(MODIFIERS).map(|p| p.to_string()).collect();
        assert_eq!(found, ours);
    }

    #[test]
    fn links_as_chapbook_reads_them() {
        let targets = |m: &str| links(m).into_iter().map(|l| l.target).collect::<Vec<_>>();
        assert_eq!(targets("[[Wait ]] [[a|b->c]] [[a->b->c]] [[x<-y]] [[]]"), ["Wait ", "b->c", "b->c", "x"]);
    }

    #[test]
    fn inserts_variables_and_their_passages() {
        let p = read(
            "Go {link to: 'Hall', label: 'the hall'} or {embed passage named: \"Doors\"}. {name} has {stats.str}.\n{reveal link: 'Look', passage: 'View'} {back lnk}",
        );
        let summary: Vec<(String, bool)> = p.inserts.iter().map(|i| (i.name.clone(), i.variable)).collect();
        assert_eq!(
            summary,
            [
                ("link to".into(), false),
                ("embed passage named".into(), false),
                ("name".into(), true),
                ("stats.str".into(), true),
                ("reveal link".into(), false),
                ("back lnk".into(), false)
            ]
        );
        assert_eq!(p.inserts[0].passage(), Some(("'Hall'", false)));
        assert_eq!(p.inserts[1].passage(), Some(("\"Doors\"", true)));
        assert_eq!(p.inserts[4].passage(), Some(("'View'", true)));
        assert!(is_insert("Embed  Passage") && !is_insert("back lnk"));
    }

    #[test]
    fn the_vars_section_and_modifiers() {
        let p = read(
            "strength: 18\nhero.name: 'Ava'\nlucky (strength > 10): true\n--\n[if strength > 10; append]\nStrong.\n[JavaScript]\nconst x = {a: 1};\n[continue]\nDone {strength}.",
        );
        let sets: Vec<&str> = p.sets.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(sets, ["strength", "hero", "lucky"]);
        assert_eq!(p.modifiers.iter().map(|(_, m)| m.clone()).collect::<Vec<_>>(), [vec!["if strength > 10", "append"], vec!["JavaScript"], vec!["continue"]]);
        assert_eq!(p.inserts.len(), 1, "the JavaScript's braces aren't inserts: {:?}", p.inserts);
        assert!(read("Not: a vars section\nbecause there's no dashes").sets.is_empty());
        let commented = read("// Characters\nhero: { name: 'Ava' }\n\nmood (hero): 'calm'\n--\nText.");
        assert_eq!(commented.sets.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["hero", "mood"], "lines with no colon are ignored");
        assert!(read("  [if x]\n[a]\n").modifiers.is_empty(), "indented, or too short, is text");
        assert!(is_modifier("note to myself") && is_modifier("cont'd") && !is_modifier("iff x"));
    }
}
