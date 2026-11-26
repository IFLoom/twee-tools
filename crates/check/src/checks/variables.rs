//! Story variables read and never set, set and never read, and properties nothing gives them.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::*;

pub(crate) fn variables(ix: &Index, config: &Config, out: &mut Out) {
    // Name -> (first set, first read), each a (passage index, offset).
    let mut sets: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut reads: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut from_code: HashSet<String> = HashSet::new();
    let mut patterns: Vec<NamePattern> = vec![];
    let mut computed = false;
    let mut js = |text: &str, into: &mut HashSet<String>| {
        let (names, built, any_computed) = js_variables_and_patterns(text);
        into.extend(names);
        patterns.extend(built);
        computed |= any_computed;
    };
    for code in &ix.story.scripts {
        js(&code.text, &mut from_code);
    }
    let codes: Vec<&str> = ix.passages().map(|(_, _, parse)| parse.code.as_str()).collect();
    for code in &codes {
        js(code, &mut from_code);
    }
    let all_code: String = ix.story.scripts.iter().map(|c| c.text.as_str()).chain(codes.iter().copied()).collect::<Vec<_>>().join("\n");
    let keys = if computed { keys_in(&all_code) } else { HashSet::new() };
    for (index, code) in codes.iter().enumerate() {
        for (offset, name, set, read) in sugarcube_variables(code) {
            if set {
                sets.entry(name.clone()).or_insert((index, offset));
            }
            if read {
                reads.entry(name).or_insert((index, offset));
            }
        }
    }
    let all: BTreeSet<&String> = sets.keys().chain(reads.keys()).collect();
    out.stats.variables = all.len();
    let passages = &ix.story.passages;
    let mut found = vec![];
    for (name, &(index, offset)) in &reads {
        // A widget sets $args itself: the older name for _args, still supported.
        let widget_args = name == "args" && passages[index].has_tag("widget");
        // With computed writes, `V[k] = …`, a name the code writes as a key or a string may be set that way: `{ofReq1Choice: 'yes'}`.
        let maybe_written = computed && if name.bytes().all(key_byte) { keys.contains(name.as_str()) } else { mentioned_as_key(name, &all_code) };
        if !sets.contains_key(name) && !from_code.contains(name) && !fits(name, &patterns) && !config.globals.contains(name) && !widget_args && !maybe_written {
            found.push((index, offset, "never-set", name.clone(), format!("${name} is read, and nothing sets it")));
        }
    }
    for (name, &(index, offset)) in &sets {
        if !reads.contains_key(name) && !from_code.contains(name) && !fits(name, &patterns) {
            found.push((index, offset, "never-read", name.clone(), format!("${name} is set, and nothing reads it")));
        }
    }
    let set_names: Vec<String> = sets.keys().cloned().collect();
    for (index, offset, code, name, message) in found {
        let severity = if code == "never-read" { Severity::Note } else { Severity::Warning };
        let finding = out.at(severity, code, &passages[index], offset, message);
        finding.subject = Some(format!("${name}"));
        if code == "never-set" {
            finding.hint = props::closest(&name, &set_names).map(|n| format!("did you mean ${n}?"));
        }
    }

    // Properties: $office.reputation read, and set by nothing (no literal holds it, no <<set>> sets it).
    let mut known = props::Known::default();
    let mut reads: Vec<(usize, props::Use)> = vec![];
    for (index, (_, _passage, parse)) in ix.passages().enumerate() {
        for found in props::uses(&parse.code) {
            if found.set {
                known.set(&found);
            }
            if found.read && found.path.len() > 1 {
                reads.push((index, found));
            }
        }
    }
    for name in &from_code {
        known.opaque(name);
    }
    for name in &config.globals {
        known.opaque(name);
    }
    let mut reported: HashSet<String> = HashSet::new();
    for (index, found) in reads {
        let Some(missing) = known.missing(&found.path) else {
            continue;
        };
        if !reported.insert(missing.clone()) {
            continue;
        }
        let last = missing.rsplit('.').next().unwrap_or(&missing).to_string();
        let siblings: Vec<String> = known.siblings(&missing).iter().map(|s| s.rsplit('.').next().unwrap_or(s).to_string()).collect();
        let parent = missing.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
        let hint = props::closest(&last, &siblings)
            .map(|n| format!("did you mean ${parent}.{n}?"))
            .or_else(|| known.same_name_under(&missing).first().map(|p| format!("did you mean ${p}?")));
        let message = format!(
            "${missing} is read, and nothing sets it: no <<set>> and no object literal gives ${} that property",
            missing.rsplit_once('.').map(|(p, _)| p).unwrap_or("")
        );
        let finding = out.at(Severity::Warning, "never-set", &passages[index], found.offset, message);
        finding.subject = Some(format!("${missing}"));
        finding.hint = hint;
    }
}
