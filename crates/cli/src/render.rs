//! The checker's report for people: the story at a glance, then its problems grouped by kind, errors first. Colour when the
//! output is a terminal; plain text otherwise, or with NO_COLOR.

use std::collections::BTreeMap;
use std::fmt::Write;

use check::{Finding, Report, Severity};
use story::Story;

/// ANSI styles, or nothing.
pub struct Style {
    on: bool,
}

impl Style {
    pub fn new(on: bool) -> Style {
        Style { on }
    }

    fn paint(&self, codes: &str, text: &str) -> String {
        if self.on { format!("\x1b[{codes}m{text}\x1b[0m") } else { text.to_string() }
    }

    fn bold(&self, t: &str) -> String {
        self.paint("1", t)
    }
    fn dim(&self, t: &str) -> String {
        self.paint("2", t)
    }
    fn red(&self, t: &str) -> String {
        self.paint("1;31", t)
    }
    fn yellow(&self, t: &str) -> String {
        self.paint("1;33", t)
    }
    fn green(&self, t: &str) -> String {
        self.paint("32", t)
    }
    fn cyan(&self, t: &str) -> String {
        self.paint("36", t)
    }

    /// A green tick, for a job done.
    pub fn green_mark(&self) -> String {
        self.green("✔")
    }
}

/// Each kind of problem: its heading, and whether its findings are a list (one line each) rather than full detail.
const KINDS: &[(&str, &str, bool)] = &[
    ("start", "Start passage", false),
    ("ifid", "IFID", false),
    ("story-format", "Story format", false),
    ("format-version", "Features newer than the story's SugarCube version", false),
    ("story-title", "Title", false),
    ("story-data", "StoryData", false),
    ("html-structure", "Published HTML", false),
    ("twee-header", "Twee passage headers", false),
    ("passage-name", "Passage names", false),
    ("broken-link", "Broken links", false),
    ("duplicate-passage", "Passages declared twice", false),
    ("js-syntax", "JavaScript that doesn't parse", false),
    ("removed-api", "Removed in SugarCube", false),
    ("special-name", "Special names written almost right", false),
    ("code-passage", "Code passages", false),
    ("twine1-tags", "Twine 1 tags in a Twine 2 story", false),
    ("macro-name", "Widget and macro names", false),
    ("config", "Config and Setting", false),
    ("wrong-format", "Another story format's markup", false),
    ("if-assignment", "Assignments in conditions", false),
    ("receiver", "Receivers not quoted", false),
    ("macro-args", "Macro arguments", false),
    ("switch", "<<switch>> and <<default>>", false),
    ("variable-name", "Names that aren't story variables", false),
    ("capture", "Loops that need <<capture>>", false),
    ("audio", "Audio", false),
    ("unknown-macro", "Unknown macros and inserts", false),
    ("unknown-modifier", "Unknown modifiers", true),
    ("widget-as-function", "Widgets called as functions", false),
    ("unclosed-macro", "Macros never closed", false),
    ("stray-close", "Closing tags with nothing open", false),
    ("misplaced-macro", "Macros in the wrong place", false),
    ("unclosed-markup", "Links, macros and hooks left open", false),
    ("else-without-hook", "(else:) or [else] with nothing before it", false),
    ("absolute-media", "Media paths from this computer", false),
    ("missing-media", "Missing media", false),
    ("unreachable", "Passages nothing leads to", true),
    ("unwritten", "Passages not written yet", true),
    ("dead-end", "Dead ends: passages with no way forward", true),
    ("unused-macro", "Widgets and macros nothing calls", true),
    ("never-set", "Variables read, and never set", true),
    ("never-read", "Variables set, and never read", true),
    ("deprecated", "Deprecated by the story format", true),
    ("naked-variable", "Expressions in naked variables", true),
    ("def-precedence", "def and ndef without parentheses", true),
    ("goto-continues", "Code after <<goto>>", true),
    ("link-target", "Link targets and setters", true),
    ("no-output", "Words in passages that show nothing", false),
    ("unused-media", "Media nothing uses", true),
    ("misnamed-media", "Media with the wrong extension", true),
    ("large-media", "Large media files", true),
];

pub struct Options {
    pub all: bool,
    pub width: usize,
}

/// How many entries a list shows before "… and N more".
const LIST_LIMIT: usize = 12;
/// How many unused files show, grouped by folder, before "… and N more".
const UNUSED_LIMIT: usize = 30;

pub fn report(story: &Story, report: &Report, style: &Style, options: &Options) -> String {
    let mut out = String::new();
    let s = &report.stats;
    let title = story.title.as_deref().unwrap_or("Untitled story");
    let format = match (&story.format, &story.format_version) {
        (Some(f), Some(v)) => format!("{f} {v}"),
        (Some(f), None) => f.clone(),
        _ => "an unknown story format".into(),
    };
    let _ = writeln!(out, "\n {}  {}", style.bold(title), style.dim(&format));
    let mut facts = [
        format!("{} passages", s.passages),
        format!("{} links", s.links),
        format!("{} macros", s.macros),
        format!("{} variables", s.variables),
        format!("{} media files", s.media_files),
    ];
    if s.dynamic_links > 0 {
        facts[1] = format!("{} links ({} built as it runs)", s.links, s.dynamic_links);
    }
    let _ = writeln!(out, " {}", style.dim(&facts.join(" · ")));
    if story.form == story::Form::Twee {
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut read = vec![plural(story.files.len(), "Twee file", "Twee files")];
        if !story.scripts.is_empty() {
            read.push(plural(story.scripts.len(), "script", "scripts"));
        }
        if !story.stylesheets.is_empty() {
            read.push(plural(story.stylesheets.len(), "stylesheet", "stylesheets"));
        }
        let _ = writeln!(out, " {}", style.dim(&format!("read from {}", read.join(" · "))));
    }
    if s.reached_by_pattern > 0 {
        let n = s.reached_by_pattern;
        let (noun, verb) = if n == 1 { ("passage", "is") } else { ("passages", "are") };
        let _ = writeln!(out, " {}", style.dim(&format!("{n} {noun} {verb} counted as reached because they're numbered like one that is")));
    }

    let m = &report.media;
    if m.files > 0 {
        let kinds: Vec<String> = m.kinds.iter().take(4).map(|(k, _, b)| format!("{k} {}", check::megabytes(*b))).collect();
        let _ = writeln!(out, "\n {}  {} in {} files · {}", style.bold("Media"), check::megabytes(m.bytes), m.files, style.dim(&kinds.join(" · ")));
        if m.compressible_bytes >= 1024 * 1024 {
            let _ = writeln!(
                out,
                "        {}",
                style.dim(&format!("{} of it is GIF, PNG or JPEG, which `twee-tools compress` re-encodes as WebP", check::megabytes(m.compressible_bytes)))
            );
        }
    }

    let errors = report.errors();
    let warnings = report.findings.iter().filter(|f| f.severity == Severity::Warning).count();
    let notes = report.findings.iter().filter(|f| f.severity == Severity::Note).count();
    if report.findings.is_empty() {
        let _ = writeln!(out, "\n {} {}\n", style.green("✔"), style.bold("No problems found."));
        return out;
    }

    for (severity, heading) in [(Severity::Error, "Errors"), (Severity::Warning, "Warnings"), (Severity::Note, "Notes")] {
        let of: Vec<&Finding> = report.findings.iter().filter(|f| f.severity == severity).collect();
        if of.is_empty() {
            continue;
        }
        let painted = match severity {
            Severity::Error => style.red(&heading.to_uppercase()),
            Severity::Warning => style.yellow(&heading.to_uppercase()),
            Severity::Note => style.bold(&heading.to_uppercase()),
        };
        let _ = writeln!(out, "\n {painted}");
        for &(code, title, list) in KINDS.iter().chain([("", "Other", false)].iter()) {
            let group: Vec<&Finding> = of.iter().copied().filter(|f| f.code == code || (code.is_empty() && !KINDS.iter().any(|k| k.0 == f.code))).collect();
            if group.is_empty() {
                continue;
            }
            let icon = match severity {
                Severity::Error => style.red("✖"),
                Severity::Warning => style.yellow("⚠"),
                Severity::Note => style.dim("·"),
            };
            let _ = writeln!(out, "\n {icon} {} {}", style.bold(title), style.dim(&format!("· {}", group.len())));
            if code == "never-read" {
                let _ =
                    writeln!(out, "   {}", style.dim("state the story records and never uses: usually harmless leftovers, sometimes a feature not finished"));
            }
            if code == "unused-media" {
                unused(&mut out, story, &group, style, options);
            } else if code == "large-media" {
                large(&mut out, &group, style, options);
            } else if list || (code == "missing-media" && severity == Severity::Warning) {
                listed(&mut out, story, &group, style, options);
            } else {
                for finding in group {
                    detailed(&mut out, story, finding, style);
                }
            }
        }
    }

    let mut tally = vec![];
    if errors > 0 {
        tally.push(style.red(&format!("✖ {errors} error{}", if errors == 1 { "" } else { "s" })));
    }
    if warnings > 0 {
        tally.push(style.yellow(&format!("⚠ {warnings} warning{}", if warnings == 1 { "" } else { "s" })));
    }
    if notes > 0 {
        tally.push(style.dim(&format!("· {notes} note{}", if notes == 1 { "" } else { "s" })));
    }
    let _ = writeln!(out, "\n {}\n", tally.join("   "));
    out
}

/// Where a finding is, short: the file's path inside the story's folder, and the line.
fn place(story: &Story, finding: &Finding) -> String {
    let file = finding.file.strip_prefix(&story.root).unwrap_or(&finding.file);
    let file = file.to_string_lossy().replace('\\', "/");
    if finding.line > 0 { format!("{file}:{}", finding.line) } else { file }
}

/// Full detail: the passage and where, the message, then any hint.
fn detailed(out: &mut String, story: &Story, finding: &Finding, style: &Style) {
    let head = match &finding.passage {
        Some(passage) => format!("{}  {}", style.cyan(passage), style.dim(&place(story, finding))),
        None => style.dim(&place(story, finding)),
    };
    let _ = writeln!(out, "   {head}");
    let _ = writeln!(out, "     {}", finding.message);
    if let Some(hint) = &finding.hint {
        let _ = writeln!(out, "     {} {}", style.green("→"), hint);
    }
}

/// One line each: the subject, padded, then where. A hint every entry shares is said once, under the heading.
fn listed(out: &mut String, story: &Story, group: &[&Finding], style: &Style, options: &Options) {
    // A note every entry shares, said once under the heading (passages nothing leads to: "unless through a built link").
    let shared = group.first().and_then(|f| f.hint.clone()).filter(|h| group.len() > 1 && group.iter().all(|f| f.hint.as_ref() == Some(h)));
    if let Some(hint) = &shared {
        let _ = writeln!(out, "   {}", style.dim(&format!("{} {hint}", "→")));
    }
    let shown = if options.all { group.len() } else { group.len().min(LIST_LIMIT) };
    let subject = |f: &Finding| f.subject.clone().or_else(|| f.passage.clone()).unwrap_or_default();
    let width = group[..shown].iter().map(|f| subject(f).chars().count()).max().unwrap_or(0).min(40);
    for finding in &group[..shown] {
        let name = subject(finding);
        let pad = " ".repeat(width.saturating_sub(name.chars().count()));
        let mut where_ = place(story, finding);
        if let Some(passage) = finding.passage.as_ref().filter(|p| **p != name) {
            where_ = format!("in {passage} · {where_}");
        }
        let own_hint = finding.hint.as_ref().filter(|_| shared.is_none()).map(|h| format!("  {} {h}", style.green("→"))).unwrap_or_default();
        let shown_where = if finding.line == 0 && finding.passage.is_none() { String::new() } else { style.dim(&where_) };
        let line = if shown_where.is_empty() {
            format!("   {}{own_hint}", style.cyan(&name))
        } else {
            format!("   {}{pad}  {shown_where}{own_hint}", style.cyan(&name))
        };
        let _ = writeln!(out, "{}", line.trim_end());
    }
    more(out, group.len() - shown, style);
}

/// Unused files, by folder, their names wrapped to the terminal.
fn unused(out: &mut String, story: &Story, group: &[&Finding], style: &Style, options: &Options) {
    let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for finding in group {
        let path = finding.subject.clone().unwrap_or_else(|| place(story, finding));
        let (folder, name) = path.rsplit_once('/').map(|(d, n)| (format!("{d}/"), n.to_string())).unwrap_or(("./".into(), path.clone()));
        folders.entry(folder).or_default().push(name);
    }
    let mut left = if options.all { usize::MAX } else { UNUSED_LIMIT };
    let mut hidden = 0;
    for (folder, names) in folders {
        if left == 0 {
            hidden += names.len();
            continue;
        }
        let take = names.len().min(left);
        left -= take;
        hidden += names.len() - take;
        let _ = writeln!(out, "   {}", style.cyan(&folder));
        let mut line = String::from("     ");
        let mut length = 5;
        for name in &names[..take] {
            if length + name.chars().count() + 2 > options.width && length > 5 {
                let _ = writeln!(out, "{}", line.trim_end());
                line = String::from("     ");
                length = 5;
            }
            line.push_str(name);
            line.push_str("  ");
            length += name.chars().count() + 2;
        }
        let _ = writeln!(out, "{}", line.trim_end());
    }
    more(out, hidden, style);
}

/// Large files, biggest first, the size in a column; then what `compress` can do about them.
fn large(out: &mut String, group: &[&Finding], style: &Style, options: &Options) {
    let mut files: Vec<&&Finding> = group.iter().collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.bytes.unwrap_or(0)));
    let shown = if options.all { files.len() } else { files.len().min(LIST_LIMIT) };
    for f in &files[..shown] {
        let size = check::megabytes(f.bytes.unwrap_or(0));
        let kind = f.hint.as_deref().unwrap_or("");
        let _ = writeln!(out, "   {:>8}  {}  {}", size, style.cyan(f.subject.as_deref().unwrap_or("")), style.dim(kind));
    }
    more(out, files.len() - shown, style);
    let images = files.iter().filter(|f| matches!(f.hint.as_deref(), Some("GIF" | "PNG" | "JPEG" | "BMP"))).count();
    let videos = files.iter().filter(|f| matches!(f.hint.as_deref(), Some("MP4" | "WebM" | "MOV" | "M4V"))).count();
    if images > 0 {
        let _ = writeln!(
            out,
            "   {} {images} of these are GIF, PNG or JPEG: `twee-tools compress` re-encodes them as WebP, usually a third of the size or less",
            style.green("→")
        );
    }
    if videos > 0 {
        let what = if videos == 1 {
            "1 is a video: a lower bitrate or resolution would shrink it".to_string()
        } else {
            format!("{videos} are videos: a lower bitrate or resolution would shrink them")
        };
        let _ = writeln!(out, "   {} {what}", style.green("→"));
    }
}

fn more(out: &mut String, hidden: usize, style: &Style) {
    if hidden > 0 {
        let _ = writeln!(out, "   {}", style.dim(&format!("… and {hidden} more (--all shows every one)")));
    }
}
