//! The story's link graph: every place a passage links somewhere, where each passage leads, what it includes, and where
//! the story starts. Built once from the index; the link, reachability and dead-end checks report from it.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::format::Format;
use crate::harlowe;
use crate::index::{Index, PassageId, named};
use crate::scan::{self, Arg};
use crate::{chapbook, snowman};

/// How a passage links somewhere: words in the messages, "The {how} goes to …".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum How {
    Link,
    ImageLink,
    /// A macro that goes to a passage: `<<goto>>`, `<<link "x" "Passage">>`, …
    Macro,
    /// `<<include>>` and `<<display>>`.
    Include,
    /// A Chapbook insert that goes to a passage: `{link to: 'Hall'}`.
    Insert,
    DataPassage,
    /// `Story.get("X")`, `Engine.play("X")` in code.
    Code,
}

impl How {
    pub(crate) fn word(self) -> &'static str {
        match self {
            How::Link => "link",
            How::ImageLink => "image link",
            How::Macro => "macro",
            How::Insert => "insert",
            How::Include => "include",
            How::DataPassage => "data-passage",
            How::Code => "code",
        }
    }
}

/// One place a passage links somewhere, as written: the target may be no passage, or one only known as the story runs.
#[derive(Debug, Clone)]
pub(crate) struct Site {
    pub(crate) passage: PassageId,
    pub(crate) offset: usize,
    pub(crate) target: String,
    pub(crate) how: How,
}

pub(crate) struct LinkGraph {
    /// Every link site, passage by passage in the story's order.
    pub(crate) sites: Vec<Site>,
    /// The passages each passage leads to: its sites' targets that are passages, and passages its code names in strings.
    pub(crate) edges: HashMap<String, BTreeSet<String>>,
    /// How many sites go somewhere only known as the story runs (`[[Go|$next]]`).
    pub(crate) dynamic: usize,
    /// The passages each passage includes with `<<include>>` or `<<display>>`.
    pub(crate) includes: HashMap<String, Vec<String>>,
    /// Where the story starts: the start passage (if it's there), SugarCube's special passages, and passages the story's
    /// JavaScript names.
    pub(crate) roots: Vec<String>,
}

/// The passages the start reaches, and how many of them only because they're numbered like one it reaches.
pub(crate) struct Reached {
    pub(crate) passages: HashSet<String>,
    pub(crate) by_pattern: usize,
}

/// Whether an argument carries on the expression before it: `+ $n`, `+$n`, `.toLowerCase()`.
fn continues(next: Option<&Arg>) -> bool {
    matches!(next, Some(Arg::Bare(word)) if word.trim_start().starts_with(['+', '.']))
}

/// Arguments as they were written, more or less: strings quoted, the rest as they are.
fn written(args: &[Arg]) -> String {
    let words: Vec<String> = args
        .iter()
        .map(|arg| match arg {
            Arg::Str(s) => format!("\"{s}\""),
            Arg::Bare(word) => word.clone(),
            Arg::Link(_, target) | Arg::Image(_, Some(target)) => target.clone(),
            Arg::Image(src, None) => src.clone(),
        })
        .collect();
    words.join(" ")
}

impl LinkGraph {
    pub(crate) fn build(ix: &Index) -> LinkGraph {
        let names = &ix.names.set;
        let mut graph = LinkGraph { sites: vec![], edges: HashMap::new(), dynamic: 0, includes: HashMap::new(), roots: vec![] };
        for (id, passage, parse) in ix.passages() {
            let markup = &parse.markup;
            let code = &parse.code;
            let mut add = |offset: usize, target: String, how: How| graph.sites.push(Site { passage: id, offset, target, how });
            let (links, images) = match ix.format {
                Format::Harlowe => (harlowe::links(markup), vec![]),
                Format::Chapbook => (chapbook::links(markup), vec![]),
                Format::Snowman => (snowman::links(markup), vec![]),
                _ => scan::links(markup),
            };
            for link in &links {
                add(link.start, link.target.clone(), How::Link);
            }
            for image in &images {
                if let Some(link) = &image.link {
                    add(image.start, link.clone(), How::ImageLink);
                }
            }
            {
                for tag in parse.tags.iter().filter(|t| !t.closing) {
                    let args = scan::args(&tag.args);
                    let how = if matches!(tag.name.as_str(), "include" | "display") { How::Include } else { How::Macro };
                    for &index in ix.format.passage_args(&tag.name, args.len()) {
                        match args.get(index) {
                            // A string the expression goes on from, `"Day" + $n`: the passage is only known as the story runs.
                            Some(Arg::Str(_)) if continues(args.get(index + 1)) => add(tag.start, written(&args[index..]), how),
                            Some(Arg::Str(name)) => add(tag.start, name.clone(), how),
                            // <<textbox "$x" "" autofocus>>: a keyword, not a passage; and these take a passage only quoted.
                            Some(Arg::Bare(word)) if word == "autofocus" => {}
                            Some(Arg::Bare(_)) if matches!(tag.name.as_str(), "back" | "return" | "textbox" | "numberbox") => {}
                            Some(Arg::Bare(word)) if scan::is_plain_word(word) => add(tag.start, word.clone(), how),
                            Some(Arg::Bare(_)) => add(tag.start, "$".into(), how),
                            _ => {}
                        }
                    }
                    for arg in &args {
                        if let Arg::Link(_, target) | Arg::Image(_, Some(target)) = arg {
                            add(tag.start, target.clone(), how);
                        }
                    }
                }
            }
            // Harlowe's macros that go to or show a passage: (go-to: "Hall"), (link-goto: "Go", "Hall"), (display: "Map").
            for call in parse.calls.iter().filter(|c| c.closed && !c.is_custom()) {
                let args = harlowe::args(&call.args);
                let Some(index) = harlowe::passage_arg(&call.name, args.len()) else { continue };
                let Some(arg) = args.get(index) else { continue };
                let how = if harlowe::normalise(&call.name) == "display" { How::Include } else { How::Macro };
                match harlowe::literal(arg) {
                    Some(name) => add(call.start, name, how),
                    // A passage worked out as the story runs: `(go-to: $next)`, `(go-to: "Day" + $n)`.
                    None => add(call.start, arg.clone(), how),
                }
            }
            // <a data-passage="X">.
            let lower = markup.to_ascii_lowercase();
            for (at, _) in lower.match_indices("data-init-passage=").chain(lower.match_indices("data-passage=")) {
                if let Some((_, value, _)) = scan::strings(&markup[at..]).first() {
                    add(at, value.clone(), How::DataPassage);
                }
            }
            // Chapbook's inserts: {link to: 'Hall'}, {embed passage: 'Map'}, {reveal link: 'Look', passage: 'View'}.
            if ix.format == Format::Chapbook {
                let read = chapbook::read(&passage.text);
                // The header and footer it sets show on every passage: their links are ways there.
                for (at, text) in &read.frame {
                    for link in chapbook::links(text) {
                        add(*at, link.target, How::Link);
                    }
                }
                let mut framed = vec![];
                for (at, text) in &read.frame {
                    chapbook::inserts(text, *at, &mut framed);
                }
                for insert in read.inserts.into_iter().chain(framed) {
                    let Some((written, in_place)) = insert.passage() else { continue };
                    let how = if in_place { How::Include } else { How::Insert };
                    let name = chapbook::literal(written);
                    if in_place && let Some(name) = &name {
                        graph.includes.entry(passage.name.clone()).or_default().push(name.clone());
                    }
                    add(insert.start, name.unwrap_or_else(|| written.to_string()), how);
                }
            }
            // Snowman's calls: story.show("Hall"), Story.goto("Hall"), story.render("Map"), hasVisited("Den").
            if ix.format == Format::Snowman {
                for named in snowman::named(code) {
                    let how = if named.does == snowman::Does::Show { How::Include } else { How::Code };
                    if named.literal && named.does == snowman::Does::Show {
                        graph.includes.entry(passage.name.clone()).or_default().push(named.passage.clone());
                    }
                    add(named.offset, named.passage, how);
                }
            }
            // Story.get("X") and Engine.play("X") in SugarCube's code.
            for needle in ["Story.get(", "Engine.play(", "Engine.goTo("].into_iter().filter(|_| ix.format.sugarcube()) {
                for (at, _) in code.match_indices(needle) {
                    if let Some((offset, value, _)) = scan::strings(&code[at + needle.len()..]).into_iter().next().filter(|(o, _, _)| *o == 1) {
                        let after = &code[at + needle.len() + offset + value.len() + 1..];
                        if after.trim_start().starts_with([')', ',']) {
                            add(at, value, How::Code);
                        }
                    }
                }
            }
            // The includes, in any story with SugarCube's macros: what a passage shows of another.
            for call in parse.calls.iter().filter(|c| c.closed && harlowe::normalise(&c.name) == "display") {
                if let Some(name) = harlowe::args(&call.args).first().and_then(|a| harlowe::literal(a)) {
                    graph.includes.entry(passage.name.clone()).or_default().push(name);
                }
            }
            for tag in parse.tags.iter().filter(|t| matches!(t.name.as_str(), "include" | "display") && !t.closing) {
                if let Some(Arg::Str(name) | Arg::Bare(name) | Arg::Link(_, name)) = scan::args(&tag.args).first() {
                    graph.includes.entry(passage.name.clone()).or_default().push(name.clone());
                }
            }
            let leads = graph.edges.entry(passage.name.clone()).or_default();
            for site in graph.sites.iter().rev().take_while(|s| s.passage == id) {
                if names.contains(site.target.as_str()) {
                    leads.insert(site.target.clone());
                } else if crate::is_dynamic(&site.target) {
                    graph.dynamic += 1;
                }
            }
            // A string anywhere that names a passage is a way there too: <<set $next to "Room">><<goto $next>>, Engine.play("Room").
            leads.extend(named(code, &ix.names));
        }
        if let Some(start) = ix.story.start.as_ref().filter(|s| names.contains(s.as_str())) {
            graph.roots.push(start.clone());
        }
        for (_, passage, _) in ix.passages() {
            if ix.format.is_special(passage) {
                graph.roots.push(passage.name.clone());
            }
        }
        for code in &ix.story.scripts {
            graph.roots.extend(named(&code.text, &ix.names));
        }
        // Harlowe choosing passages by their tags, `(passages: where its tags contains "Story-Start")`: any passage with
        // such a tag can be reached that way.
        if ix.format == Format::Harlowe {
            let by_tag = regex::Regex::new(r#"tags\s+contains\s+["']([^"']+)["']"#).unwrap();
            let chosen: std::collections::HashSet<String> =
                ix.passages().flat_map(|(_, _, parse)| by_tag.captures_iter(&parse.code).map(|c| c[1].to_string()).collect::<Vec<_>>()).collect();
            for (_, passage, _) in ix.passages() {
                if passage.tags.iter().any(|t| chosen.contains(t)) {
                    graph.roots.push(passage.name.clone());
                }
            }
        }
        // Harlowe's storylets: passages it offers by their (storylet:) condition, through (link-storylet:) or
        // (open-storylets:), not by name.
        for (_, passage, parse) in ix.passages() {
            if parse.calls.iter().any(|c| harlowe::normalise(&c.name) == "storylet") {
                graph.roots.push(passage.name.clone());
            }
        }
        graph
    }

    /// The passages the roots lead to, or nothing to go on when there are no roots. With any dynamic links, a passage
    /// numbered like one that's reached (the same name but for its digits: Week3.1, Week4.1) counts as reached too.
    pub(crate) fn reached(&self, ix: &Index) -> Option<Reached> {
        if self.roots.is_empty() {
            return None;
        }
        let mut seen: HashSet<String> = self.roots.iter().cloned().collect();
        self.follow(self.roots.iter().cloned().collect(), &mut seen);
        let mut by_pattern = 0;
        if self.dynamic > 0 {
            let pattern = |name: &str| -> Option<String> {
                name.chars().any(|c| c.is_ascii_digit()).then(|| name.chars().map(|c| if c.is_ascii_digit() { '9' } else { c }).collect::<String>())
            };
            loop {
                let reached: HashSet<String> = seen.iter().filter_map(|n| pattern(n)).collect();
                let more: Vec<String> = ix
                    .passages()
                    .map(|(_, p, _)| p)
                    .filter(|p| !seen.contains(&p.name) && pattern(&p.name).is_some_and(|k| reached.contains(&k)))
                    .map(|p| p.name.clone())
                    .collect();
                if more.is_empty() {
                    break;
                }
                by_pattern += more.len();
                let queue: VecDeque<String> = more.into_iter().filter(|n| seen.insert(n.clone())).collect();
                self.follow(queue, &mut seen);
            }
        }
        Some(Reached { passages: seen, by_pattern })
    }

    fn follow(&self, mut queue: VecDeque<String>, seen: &mut HashSet<String>) {
        while let Some(name) = queue.pop_front() {
            for next in self.edges.get(&name).into_iter().flatten() {
                if seen.insert(next.clone()) {
                    queue.push_back(next.clone());
                }
            }
        }
    }
}
