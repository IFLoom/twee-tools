//! Checks a whole Twine story: links, reachability, variables, macros and media.

mod chapbook;
mod checks;
pub mod config;
mod format;
mod graph;
mod harlowe;
mod index;
pub mod js;
mod macros_defined;
pub use story::media;
mod out;
pub mod props;
mod snowman;
mod words;
pub use story::markup as scan;
pub mod sugarcube;

use std::collections::HashSet;
use std::path::PathBuf;

use serde::Serialize;
use story::{Passage, Story, parse_version};

use crate::config::Config;
use crate::scan::Keep;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    /// Worth knowing, and seldom a bug: state the story records and never uses.
    Note,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// A short name for the kind of problem, such as `broken-link`.
    pub code: &'static str,
    pub message: String,
    pub file: PathBuf,
    pub line: usize,
    pub passage: Option<String>,
    /// What it's about, short: a passage, `$name`, `<<macro>>`, a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    /// A suggestion: "did you mean …?".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// A file's size, for large media.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
}

#[derive(Debug, Default, Serialize)]
pub struct Stats {
    pub passages: usize,
    /// The words a player reads: prose in passages the format doesn't run for itself, without code or markup.
    pub words: usize,
    pub links: usize,
    pub dynamic_links: usize,
    /// Passages counted as reached only because they're numbered like one that is.
    pub reached_by_pattern: usize,
    pub variables: usize,
    pub macros: usize,
    pub media_refs: usize,
    pub dynamic_media: usize,
    pub media_files: usize,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub findings: Vec<Finding>,
    pub stats: Stats,
    pub media: MediaSummary,
}

/// How much the story's media weighs.
#[derive(Debug, Default, Serialize)]
pub struct MediaSummary {
    pub files: usize,
    pub bytes: u64,
    /// By what each file really is (GIF, WebP, MP4, ...): (kind, files, bytes), heaviest first.
    pub kinds: Vec<(String, usize, u64)>,
    /// GIF, PNG, JPEG and BMP: what `twee-tools compress` re-encodes as WebP.
    pub compressible_bytes: u64,
}

/// A media file this big or bigger is reported.
pub const LARGE_MEDIA: u64 = 5 * 1024 * 1024;

impl Report {
    pub fn errors(&self) -> usize {
        self.findings.iter().filter(|f| f.severity == Severity::Error).count()
    }
}

/// What a long job says it's doing, for a spinner: "Checking media 120 of 2570".
pub use story::Progress;

pub fn check(story: &Story, config: &Config) -> Report {
    check_with(story, config, &|_| {})
}

/// The project's say over findings: codes it ignores, severities it changes, and passages tagged to be left alone
/// (`twee-tools-ignore` for every finding in them, `twee-tools-ignore-dead-end` for one code).
fn settle(findings: &mut Vec<Finding>, ix: &index::Index, config: &Config) {
    findings.retain(|f| {
        if config.ignore.contains(f.code) {
            return false;
        }
        let tags = f.passage.as_deref().and_then(|name| ix.named(name)).map(|id| &ix.passage(id).tags);
        !tags.is_some_and(|tags| tags.iter().any(|t| t == "twee-tools-ignore" || t.strip_prefix("twee-tools-ignore-") == Some(f.code)))
    });
    for finding in findings {
        if let Some(&severity) = config.severity.get(finding.code) {
            finding.severity = severity;
        }
    }
}

/// `check`, saying what it's doing as it goes.
///
/// The checks that don't need each other run at the same time, each into its own `Out`; their findings are joined in
/// one fixed order and then sorted, so the report is the same however the work was shared out.
pub fn check_with(story: &Story, config: &Config, progress: Progress) -> Report {
    use checks::*;
    progress(&format!("Reading {} passages", story.passages.len()));
    let ix = index::Index::new(story);
    let sugarcube = ix.format.sugarcube();
    progress("Checking links, variables, macros, markup and JavaScript, and finding media");
    let mut outs: [out::Out; 9] = Default::default();
    {
        let [data, paths, vars, macro_out, names_out, format_out, pitfalls_out, js, media_out] = &mut outs;
        let ix = &ix;
        rayon::scope(|s| {
            s.spawn(|_| {
                story_data::story_level(ix, config, data);
                structure::duplicates(ix, config, data);
                structure::unwritten(ix, config, data);
            });
            s.spawn(|_| {
                let graph = graph::LinkGraph::build(ix);
                links::broken_links(ix, config, paths, &graph);
                let reached = links::unreachable(ix, config, paths, &graph);
                if ix.format != format::Format::Other {
                    dead_ends::dead_ends(ix, config, paths, &graph, &reached);
                }
            });
            // `$name` variables are SugarCube's and Harlowe's: Chapbook has its own (in its check), Snowman's are JavaScript.
            if matches!(ix.format, format::Format::SugarCube | format::Format::Harlowe) {
                s.spawn(|_| variables::variables(ix, config, vars));
            }
            if sugarcube {
                s.spawn(|_| macros::macros(ix, config, macro_out));
                s.spawn(|_| names::sugarcube_rules(ix, config, names_out));
            }
            // The format's own rules, beyond SugarCube's above.
            match ix.format {
                format::Format::Harlowe => s.spawn(|_| harlowe::harlowe(ix, config, format_out)),
                format::Format::Chapbook => s.spawn(|_| chapbook::chapbook(ix, config, format_out)),
                format::Format::Snowman => s.spawn(|_| snowman::snowman(ix, config, format_out)),
                format::Format::SugarCube | format::Format::Other => {}
            }
            s.spawn(|_| pitfalls::pitfalls(ix, config, pitfalls_out));
            s.spawn(|_| javascript::javascript(ix, config, js));
            s.spawn(|_| media::media(ix, config, media_out, progress));
        });
    }
    let mut out = out::Out::default();
    out.stats.passages = story.passages.len();
    out.stats.words = words::count(&ix);
    for part in outs {
        out.absorb(part);
    }
    let (mut findings, stats, summary) = out.into_parts();
    settle(&mut findings, &ix, config);
    findings.sort_by(|a, b| (a.file.clone(), a.line, a.severity, a.code).cmp(&(b.file.clone(), b.line, b.severity, b.code)));
    Report { findings, stats, media: summary }
}

/// A file's kind by its extension, as people write it: WebM, MP4, MP3.
fn kind_name(file: &str) -> String {
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "webm" => "WebM".into(),
        "webp" => "WebP".into(),
        "jpg" | "jpeg" => "JPEG".into(),
        "" => "other".into(),
        other => other.to_ascii_uppercase(),
    }
}

/// Sizes in MB, or KB below one.
pub fn megabytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 { format!("{:.1} MB", bytes as f64 / 1_048_576.0) } else { format!("{} KB", bytes.div_ceil(1024)) }
}

/// One media file, from one open: (what it really is if its extension says otherwise, what it really is, its size).
fn sniff(path: &std::path::Path) -> (Option<&'static str>, Option<&'static str>, u64) {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return (None, None, 0);
    };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut head = [0u8; 12];
    let n = file.read(&mut head).unwrap_or(0);
    let actual = kind_of(&head[..n]);
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let wrong = actual.filter(|a| {
        let matches = match *a {
            "GIF" => ext == "gif",
            "PNG" => ext == "png",
            "JPEG" => matches!(ext.as_str(), "jpg" | "jpeg" | "jpe" | "jfif"),
            _ => ext == "webp",
        };
        !matches && media::IMAGE_EXTS.contains(&ext.as_str())
    });
    (wrong, actual, size)
}

/// What an image is, by its first bytes.
fn kind_of(head: &[u8]) -> Option<&'static str> {
    let actual = if head.starts_with(b"GIF8") {
        "GIF"
    } else if head.starts_with(&[0x89, b'P', b'N', b'G']) {
        "PNG"
    } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "JPEG"
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        "WebP"
    } else {
        return None;
    };
    Some(actual)
}

/// A file of the same name somewhere else in the story's folder, or one in the same folder whose name is a typo away.
fn elsewhere(path: &str, files: &[String]) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let found: Vec<&String> = files.iter().filter(|f| f.rsplit('/').next().unwrap_or(f).to_lowercase() == name).take(3).collect();
    let folder = path.rsplit_once('/').map(|(f, _)| f.to_lowercase()).unwrap_or_default();
    let close = || {
        files.iter().filter(|f| f.rsplit_once('/').map(|(d, _)| d.to_lowercase()).unwrap_or_default() == folder).find(|f| {
            let other = f.rsplit('/').next().unwrap_or(f).to_lowercase();
            other != name && edits(&other, &name) <= 2
        })
    };
    match found.as_slice() {
        [] => close().map(|f| format!("did you mean \"{f}\"?")),
        [one] => Some(format!("there's one at \"{one}\"")),
        many => Some(format!("there are files of that name at {}", many.iter().map(|f| format!("\"{f}\"")).collect::<Vec<_>>().join(", "))),
    }
}

/// How many single-character edits turn one name into the other.
fn edits(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != cb));
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

fn passage_of(place: &media::Place) -> Option<String> {
    match place {
        media::Place::Passage(name) => Some(name.clone()),
        _ => None,
    }
}

fn is_dynamic(target: &str) -> bool {
    target.starts_with(['$', '_']) || target.contains(['(', '+', '"', '\'', '`']) || target.contains("setup.")
}

fn is_external(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.contains("://") || lower.starts_with("mailto:") || lower.starts_with('#') || lower.ends_with(".html")
}

/// A passage whose name differs only by case or spaces.
fn suggest(target: &str, names: &HashSet<&str>) -> Option<String> {
    let squash = |s: &str| s.to_lowercase().split_whitespace().collect::<String>();
    let wanted = squash(target);
    // Links are case-sensitive, and Twine keeps spaces: `[[A PASSAGE]]` doesn't reach "A passage". Then a typo away.
    let mut sorted: Vec<&&str> = names.iter().collect();
    sorted.sort();
    sorted
        .iter()
        .find(|n| squash(n) == wanted)
        .map(|n| n.to_string())
        .or_else(|| {
            let owned: Vec<String> = sorted.iter().map(|n| n.to_string()).collect();
            props::closest(target, &owned).cloned()
        })
        .map(|name| format!("did you mean \"{name}\"?"))
}

fn suggest_macro(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    sugarcube::CONTAINERS.iter().chain(sugarcube::SINGLES).find(|m| **m == lower).map(|m| format!("macro names are case-sensitive: <<{m}>>"))
}

/// The `$variables` in SugarCube text: (offset, name, sets it, reads it).
/// Whether an offset is inside an `<<unset …>>`: after the last `<<unset` before it, with no `>>` between. `unsets` are
/// the text's `<<unset` offsets, found once, so each look is a search and not a scan from the start.
pub(crate) fn inside_unset(code: &str, unsets: &[usize], at: usize) -> bool {
    let before = unsets.partition_point(|&u| u < at);
    before > 0 && !code[unsets[before - 1]..at].contains(">>")
}

fn sugarcube_variables(code: &str) -> Vec<(usize, String, bool, bool)> {
    let unsets: Vec<usize> = code.match_indices("<<unset").map(|(at, _)| at).collect();
    let bytes = code.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') && (i == 0 || !is_word(bytes[i - 1])) {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && is_word(bytes[j]) {
                j += 1;
            }
            let name = code[start..j].to_string();
            let after = code[j..].trim_start_matches([' ', '\t']);
            let before = code[..i].trim_end_matches([' ', '\t']);
            let assigns =
                after.starts_with("to ") || after.starts_with("to\t") || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"));
            let updates = ["+=", "-=", "*=", "/=", "%=", "++", "--", "??=", "||=", "&&="].iter().any(|op| after.starts_with(op))
                || before.ends_with("++")
                || before.ends_with("--");
            // A receiver: <<textbox "$name" ...>>, <<checkbox "$name" ...>> and the rest; <<unset $name>>.
            let quoted_before = i > 0 && matches!(bytes[i - 1], b'"' | b'\'' | b'`');
            let receiver = quoted_before && {
                let head = before.trim_end_matches(['"', '\'', '`']).trim_end();
                ["textbox", "textarea", "numberbox", "checkbox", "radiobutton", "listbox", "cycle"].iter().any(|m| head.ends_with(&format!("<<{m}")))
            };
            let unset = inside_unset(code, &unsets, i);
            // Harlowe: (put: 5 into $gold), (move: $a into $b), (unpack: … into …); and its inputs, (input-box: bind $name).
            // By `get`, which is None at a byte inside a character (`—`), never a panic.
            let word_before = |w: &str| {
                let cut = before.len().saturating_sub(w.len());
                before.len() >= w.len()
                    && before.get(cut..).is_some_and(|tail| tail.eq_ignore_ascii_case(w))
                    && !before[..cut].ends_with(|c: char| c.is_alphanumeric() || c == '_')
            };
            let harlowe_set = word_before("into") || word_before("bind") || word_before("2bind");
            let set = assigns || updates || receiver || unset || harlowe_set;
            let read = !(assigns || receiver || unset);
            out.push((i, name, set, read));
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A name a story builds as it runs: `V['item_' + id + '_img']` is ("item_", "_img").
pub type NamePattern = (String, String);

/// Whether code writes a name as an object key (`name:`) or a quoted string (`'name'`).
/// A byte of a word as `keys_in` splits code into words.
fn key_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Every word the code writes as a key (`word:`) or as a whole string (`"word"`): what `mentioned_as_key` finds for
/// one name, found for all of them in one pass.
fn keys_in(code: &str) -> HashSet<&str> {
    let bytes = code.as_bytes();
    let mut out = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if !key_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && key_byte(bytes[i]) {
            i += 1;
        }
        let before = code[..start].chars().next_back();
        if before == Some('.') {
            continue;
        }
        let after = &code[i..];
        let key = after.trim_start().starts_with(':') && !after.trim_start().starts_with("::");
        let quoted = matches!(before, Some('"' | '\'' | '`')) && after.starts_with(before.unwrap());
        if key || quoted {
            out.insert(&code[start..i]);
        }
    }
    out
}

fn mentioned_as_key(name: &str, code: &str) -> bool {
    code.match_indices(name).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = &code[at + name.len()..];
        let word_before = before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.');
        !word_before
            && (after.trim_start().starts_with(':') && !after.trim_start().starts_with("::")
                || (matches!(before, Some('"' | '\'' | '`')) && after.starts_with(before.unwrap())))
    })
}

/// Whether a variable's name fits one the story builds.
fn fits(name: &str, patterns: &[NamePattern]) -> bool {
    patterns.iter().any(|(prefix, suffix)| name.len() >= prefix.len() + suffix.len() && name.starts_with(prefix.as_str()) && name.ends_with(suffix.as_str()))
}

/// The literal text at the start and end of a key built with `+` or a template, if there's enough to go on.
fn built_name(key: &str) -> Option<NamePattern> {
    let key = key.trim();
    let (prefix, suffix) = if key.starts_with('`') && key.ends_with('`') && key.contains("${") {
        let inner = &key[1..key.len() - 1];
        (inner[..inner.find("${").unwrap_or(0)].to_string(), inner[inner.rfind('}').map(|i| i + 1).unwrap_or(inner.len())..].to_string())
    } else if key.contains('+') {
        let parts: Vec<&str> = key.split('+').map(str::trim).collect();
        let literal = |p: &str| {
            (p.len() >= 2 && matches!(p.as_bytes()[0], b'"' | b'\'') && p.ends_with(p.chars().next().unwrap())).then(|| p[1..p.len() - 1].to_string())
        };
        (literal(parts[0]).unwrap_or_default(), literal(parts[parts.len() - 1]).unwrap_or_default())
    } else {
        return None;
    };
    let prefix = prefix.trim_start_matches('$').to_string();
    (prefix.len() + suffix.len() >= 3).then_some((prefix, suffix))
}

/// Story variables JavaScript names, and the shapes of names it builds.
fn js_variables_and_patterns(code: &str) -> (Vec<String>, Vec<NamePattern>, bool) {
    let mut out = vec![];
    let mut patterns = vec![];
    // An alias: `var sv = SugarCube.State.variables;`, then `sv.name`.
    let mut markers: Vec<String> = ["State.variables.", "variables().", "State.variables[", "variables()["].map(String::from).to_vec();
    for source in ["State.variables", "variables()"] {
        for (at, _) in code.match_indices(source) {
            let before = code[..at].trim_end_matches("SugarCube.").trim_end();
            let after = &code[at + source.len()..];
            if after.starts_with(['.', '[']) || !before.ends_with('=') || before.ends_with("==") {
                continue;
            }
            let name: String = before[..before.len() - 1]
                .trim_end()
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if !name.is_empty() && name != "variables" {
                markers.push(format!("{name}."));
                markers.push(format!("{name}["));
            }
        }
    }
    markers.sort();
    markers.dedup();
    for marker in &markers {
        let marker = marker.as_str();
        for (at, _) in code.match_indices(marker) {
            let joined = code[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
            if joined && !marker.starts_with("State.") && !marker.starts_with("variables(") {
                continue;
            }
            // A key built as the story runs: V['item_' + id + '_img'].
            if marker.ends_with('[') {
                let inside = &code[at + marker.len()..];
                let key = &inside[..inside.find(']').unwrap_or(inside.len()).min(200)];
                if let Some(pattern) = built_name(key) {
                    patterns.push(pattern);
                    continue;
                }
            }
            let rest = code[at + marker.len()..].trim_start_matches(['"', '\'', '`']);
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    for marker in ["getVar(", "setVar("] {
        for (at, _) in code.match_indices(marker) {
            let inside = &code[at + marker.len()..];
            let key = &inside[..inside.find([',', ')']).unwrap_or(inside.len()).min(200)];
            if let Some(pattern) = built_name(key) {
                patterns.push(pattern);
                continue;
            }
            let rest = inside.trim_start().trim_start_matches(['"', '\'', '`']);
            if let Some(rest) = rest.strip_prefix('$') {
                out.push(rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect());
            }
        }
    }
    // Variables read or written with a computed key, `V[key]`: then any name the code builds, `'ofReq' + id + 'Step'`, may be one.
    let aliases: Vec<String> = markers.iter().filter(|m| m.ends_with('[')).cloned().collect();
    let computed = aliases.iter().any(|marker| {
        code.match_indices(marker.as_str()).any(|(at, _)| {
            let key = code[at + marker.len()..].trim_start();
            key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && !key.starts_with("'") && !key.starts_with('"')
        })
    });
    if computed {
        for (offset, value, after) in scan::all_strings(code) {
            let ident = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            let joined_after = code[after.min(code.len())..].trim_start().starts_with('+');
            let joined_before = offset > 0 && code[..offset - 1].trim_end().ends_with('+');
            if joined_after && ident(&value) && value.len() >= 3 {
                patterns.push((value.clone(), String::new()));
            }
            if joined_before && ident(&value) && value.len() >= 3 {
                patterns.push((String::new(), value));
            }
        }
    }
    (out, patterns, computed)
}

/// Whether text can move the player on: a link, a navigation macro, data-passage, Engine.play, a [[link]] built in code.
fn has_way_out(text: &str) -> bool {
    let markup = scan::blank(text, Keep::Markup);
    let tags = scan::macros(&markup);
    way_out(text, &markup, &tags)
}

/// `has_way_out`, given the text's markup and macros already found.
fn way_out(text: &str, markup: &str, tags: &[scan::MacroTag]) -> bool {
    let (links, images) = scan::links(markup);
    if !links.is_empty() || images.iter().any(|i| i.link.is_some()) {
        return true;
    }
    let navigation =
        ["goto", "back", "return", "actions", "choice", "click", "button", "link", "linkreplace", "linkappend", "linkprepend", "include", "restart"];
    if tags.iter().any(|t| !t.closing && navigation.contains(&t.name.as_str())) {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    ["engine.play", "engine.go", "engine.backward", "engine.forward", "engine.restart", "data-passage", "[[", "ui.restart", "dialog.", "<a "]
        .iter()
        .any(|m| lower.contains(m))
}

/// Whether markup calls one of these macros.
fn calls_any(markup: &str, macros: &HashSet<String>) -> bool {
    !macros.is_empty() && scan::macros(markup).iter().any(|t| !t.closing && macros.contains(&t.name))
}

/// A passage's code alone, the same length (newlines kept): macro arguments, and the bodies of <<script>> and <script>.
fn code_only(text: &str, tags: &[scan::MacroTag]) -> String {
    let mut out: Vec<u8> = text.bytes().map(|b| if b == b'\n' { b'\n' } else { b' ' }).collect();
    let mut keep = |from: usize, to: usize| {
        let to = to.min(text.len());
        if from < to {
            out[from..to].copy_from_slice(&text.as_bytes()[from..to]);
        }
    };
    for (at, tag) in tags.iter().enumerate() {
        if tag.closing {
            continue;
        }
        keep(tag.args_start, tag.end.saturating_sub(2));
        if tag.name == "script" {
            let end = tags[at..].iter().find(|t| t.closing && t.name == "script").map(|t| t.start).unwrap_or(text.len());
            keep(tag.end, end);
        }
    }
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(open) = lower[from..].find("<script").map(|i| from + i) {
        let Some(body) = lower[open..].find('>').map(|i| open + i + 1) else { break };
        let end = lower[body..].find("</script>").map(|i| body + i).unwrap_or(text.len());
        if open == 0 || lower.as_bytes()[open - 1] != b'<' {
            keep(body, end);
        }
        from = end.max(open + 1);
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// The contents of string literals blanked, the same length: so code is searched, not the words it holds.
fn blank_strings(text: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    for (offset, value, _) in scan::strings(text) {
        for byte in &mut out[offset..offset + value.len()] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// HTML tags removed from text, kept the same length.
fn strip_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for c in text.chars() {
        match c {
            '<' => inside = true,
            '>' if inside => {
                inside = false;
                out.push(' ');
                continue;
            }
            _ => {}
        }
        out.push(if inside { ' ' } else { c });
    }
    // Entities and a bare backslash (line continuation) aren't words.
    out.replace("&nbsp;", "      ").replace('\\', " ")
}

/// Where code uses an API: the text, not as part of a longer name (`xState.play(` isn't `State.play(`).
fn find_api(text: &str, needle: &str) -> Option<usize> {
    text.match_indices(needle).map(|(at, _)| at).find(|&at| {
        let before = text[..at].chars().next_back();
        let after = text[at + needle.len()..].chars().next();
        let word = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
        let starts_with_dot = needle.starts_with('.');
        let ok_before = starts_with_dot || !before.is_some_and(|c| word(c) || c == '.');
        let ok_after = needle.ends_with(['(', '.']) || !after.is_some_and(word);
        ok_before && ok_after
    })
}

/// Two words in a row, outside strings: `is not`.
fn find_word_pair(code: &str, first: &str, second: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let chars: Vec<(usize, char)> = code.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if matches!(c, '"' | '\'' | '`') => quote = Some(c),
            None => {
                let starts_word = code[..at].chars().next_back().is_none_or(|p| !(p.is_alphanumeric() || p == '_' || p == '$' || p == '.'));
                if starts_word && code[at..].starts_with(first) {
                    let rest = &code[at + first.len()..];
                    let gap = rest.len() - rest.trim_start().len();
                    if gap > 0
                        && rest.trim_start().starts_with(second)
                        && !rest.trim_start()[second.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
                    {
                        return Some(at);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// The relative path to suggest: the file of that name in the story's folder, if there's one.
fn relative_hint(written: &str, files: &[String]) -> Option<String> {
    let name = written.rsplit(['/', '\\']).next()?.to_lowercase();
    let found = files.iter().find(|f| f.rsplit('/').next().is_some_and(|n| n.to_lowercase() == name));
    Some(match found {
        Some(file) => format!("write it relative to the story: \"{file}\""),
        None => "copy the file into the story's folder, and write its path relative to the story".into(),
    })
}

/// An uppercase version 4 UUID: 8-4-4-4-12 hex digits, the third group starting 4, the fourth 8, 9, A or B.
fn is_v4_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    let sizes = [8, 4, 4, 4, 12];
    groups.len() == 5
        && groups.iter().zip(sizes).all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c)))
        && groups[2].starts_with('4')
        && groups[3].starts_with(['8', '9', 'A', 'B'])
}

/// A passage nobody has written: empty, or Twine's text for a new one.
fn is_unwritten(passage: &Passage) -> bool {
    let text = passage.text.trim();
    text.is_empty() || text == "Double-click this passage to edit it." || text == "Double-click this passage to edit it"
}

/// An ending, by its tags or its name: a passage with nowhere to go on purpose.
fn is_ending(passage: &Passage) -> bool {
    let tagged = passage
        .tags
        .iter()
        .any(|t| matches!(t.to_ascii_lowercase().as_str(), "end" | "ending" | "the-end" | "gameover" | "game-over" | "game_over" | "final" | "death"));
    let name = passage.name.to_ascii_lowercase();
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    // The last words a player reads, which say the story is over.
    let text = passage.text.to_lowercase();
    let says_over =
        ["the end", "game over", "you died", "you have died", "you are dead", "thanks for playing", "thank you for playing"].iter().any(|w| text.contains(w));
    tagged
        || words.iter().any(|w| {
            matches!(*w, "end" | "ending" | "endings" | "gameover" | "credits" | "epilogue" | "fin" | "death" | "dead" | "victory" | "win" | "won" | "finale")
                || matches!(*w, "complete" | "completed" | "finish" | "finished")
        })
        || name.contains("game over")
        || name.contains("the end")
        || says_over
}
