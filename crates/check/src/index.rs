//! The story as every check reads it: each passage parsed once, and the passages' names indexed.

use std::collections::{BTreeSet, HashMap, HashSet};

use story::{Passage, Story};

use crate::format::Format;
use crate::macros_defined::StoryMacro;
use crate::scan::{self, Keep};

/// A passage's place in the story. Only an `Index` makes one, so it always names a passage of that story.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PassageId(usize);

/// A passage as the checks read it: its markup (code and comments blanked), its code (comments blanked), its macros.
pub(crate) struct PassageParse {
    pub(crate) markup: String,
    pub(crate) code: String,
    pub(crate) tags: Vec<scan::MacroTag>,
    /// Harlowe's macro calls, `(name: …)`, in a Harlowe story; none in any other.
    pub(crate) calls: Vec<crate::harlowe::Call>,
}

impl PassageParse {
    pub(crate) fn of(text: &str, format: Format) -> PassageParse {
        let markup = scan::blank(text, Keep::Markup);
        let tags = scan::macros(&markup);
        let calls = if format == Format::Harlowe { crate::harlowe::calls(&markup) } else { vec![] };
        PassageParse { code: scan::blank(text, Keep::Code), markup, tags, calls }
    }
}

/// A story, with every passage parsed once (in parallel) and its names indexed. Read-only: checks share one.
pub(crate) struct Index<'a> {
    pub(crate) story: &'a Story,
    pub(crate) names: Names<'a>,
    parses: Vec<PassageParse>,
    macros: Vec<StoryMacro>,
    /// The story's format: whose rules apply.
    pub(crate) format: Format,
}

impl<'a> Index<'a> {
    pub(crate) fn new(story: &'a Story) -> Index<'a> {
        use rayon::prelude::*;
        let format = Format::of(story);
        let parses: Vec<PassageParse> = story.passages.par_iter().map(|p| PassageParse::of(&p.text, format)).collect();
        let macros = crate::macros_defined::find(story, &parses);
        Index { story, names: Names::new(&story.passages), parses, macros, format }
    }

    /// Every passage, in the story's order, with its id and its parse.
    pub(crate) fn passages(&self) -> impl Iterator<Item = (PassageId, &'a Passage, &PassageParse)> + '_ {
        self.story.passages.iter().zip(&self.parses).enumerate().map(|(at, (passage, parse))| (PassageId(at), passage, parse))
    }

    pub(crate) fn passage(&self, id: PassageId) -> &'a Passage {
        &self.story.passages[id.0]
    }

    pub(crate) fn parse(&self, id: PassageId) -> &PassageParse {
        &self.parses[id.0]
    }

    /// The macros the story defines itself: widgets, then `Macro.add` in its JavaScript and in passages.
    pub(crate) fn macros(&self) -> &[StoryMacro] {
        &self.macros
    }

    /// The passage a name means: the first with that name.
    pub(crate) fn named(&self, name: &str) -> Option<PassageId> {
        self.names.index.get(name).copied()
    }
}

/// The passages a piece of text names in its strings: whole, or as the start or end of a name it builds,
/// `Engine.play('Piercing ' + part)` reaching every passage whose name starts "Piercing ".
pub(crate) fn named(text: &str, names: &Names) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (offset, value, after) in scan::all_strings(text) {
        // A whole name is reached; and if it's joined with `+`, so are the names it starts or ends ("Bar" + week: Bar and Bar3.1).
        if names.set.contains(value.as_str()) {
            out.insert(value.clone());
        }
        let joined_after = text.get(after..).is_some_and(|rest| rest.trim_start().starts_with('+'));
        let joined_before = offset > 0 && text.get(..offset - 1).is_some_and(|head| head.trim_end().ends_with('+'));
        // Only the part of the string that can be in a name: what follows the last quote or bracket.
        let name_part = |s: &str, from_end: bool| -> String {
            let stop = |c: char| matches!(c, '\'' | '"' | '`' | '(' | ')' | '<' | '>' | '[' | ']' | '{' | '}' | '\\' | '=' | ';');
            if from_end { s.rsplit(stop).next().unwrap_or("").to_string() } else { s.split(stop).next().unwrap_or("").to_string() }
        };
        if joined_after {
            let prefix = name_part(&value, true);
            if prefix.trim().len() >= 3 {
                out.extend(names.starting(&prefix).map(str::to_string));
            }
        }
        if joined_before {
            let suffix = name_part(&value, false);
            if suffix.trim().len() >= 3 {
                out.extend(names.ending(&suffix).map(str::to_string));
            }
        }
    }
    out
}

/// The passages' names, indexed once per check: which exist, which start or end with some text, and where each is.
pub(crate) struct Names<'a> {
    pub(crate) set: HashSet<&'a str>,
    sorted: BTreeSet<&'a str>,
    /// Each name reversed, to find the names that end with some text as a range.
    reversed: BTreeSet<(String, &'a str)>,
    index: HashMap<&'a str, PassageId>,
}

impl<'a> Names<'a> {
    fn new(passages: &'a [Passage]) -> Names<'a> {
        let mut index = HashMap::new();
        for (at, passage) in passages.iter().enumerate() {
            index.entry(passage.name.as_str()).or_insert(PassageId(at));
        }
        Names {
            set: passages.iter().map(|p| p.name.as_str()).collect(),
            sorted: passages.iter().map(|p| p.name.as_str()).collect(),
            reversed: passages.iter().map(|p| (p.name.chars().rev().collect(), p.name.as_str())).collect(),
            index,
        }
    }

    pub(crate) fn starting<'s>(&'s self, prefix: &'s str) -> impl Iterator<Item = &'a str> + 's {
        use std::ops::Bound;
        self.sorted.range::<str, _>((Bound::Included(prefix), Bound::Unbounded)).take_while(move |n| n.starts_with(prefix)).copied()
    }

    pub(crate) fn ending<'s>(&'s self, suffix: &str) -> impl Iterator<Item = &'a str> + 's {
        let reversed: String = suffix.chars().rev().collect();
        let from = (reversed.clone(), "");
        self.reversed.range(from..).take_while(move |(r, _)| r.starts_with(&reversed)).map(|(_, n)| *n)
    }
}
