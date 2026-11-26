//! Chapbook's own rules: inserts and modifiers it doesn't have, `[else]` with no `[if]` before it, and variables shown
//! that no vars section (or code) sets. Links and the passages its inserts name are checked by the graph.

use std::collections::HashSet;

use crate::chapbook;
use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::{Severity, props};

pub(crate) fn chapbook(ix: &Index, config: &Config, out: &mut Out) {
    let read: Vec<chapbook::Passage> = ix.passages().map(|(_, p, _)| chapbook::read(&p.text)).collect();
    let insert_names: Vec<String> = ["ambient sound", "back link", "cycling link for", "dropdown menu for", "embed Flickr image", "embed image"]
        .into_iter()
        .chain(["embed passage named", "embed YouTube video", "link to", "restart link", "reveal link", "sound effect", "text input for", "theme switcher"])
        .map(str::to_string)
        .collect();
    let modifier_names: Vec<String> =
        ["after", "align center", "append", "if", "else", "unless", "continue", "CSS", "JavaScript", "note"].into_iter().map(str::to_string).collect();

    // What sets a variable: vars sections, inserts that take input, and engine.state.set('name', …) in code.
    let mut set: HashSet<String> = read.iter().flat_map(|p| p.sets.iter().map(|(n, _)| n.clone())).collect();
    for passage in &read {
        for insert in passage.inserts.iter().filter(|i| !i.variable) {
            if let Some(name) = input_variable(insert) {
                set.insert(name);
            }
        }
    }
    let code: Vec<&str> = ix.story.scripts.iter().map(|c| c.text.as_str()).chain(ix.passages().map(|(_, p, _)| p.text.as_str())).collect();
    let custom = chapbook::Custom::of(code.iter().copied());
    let mut opaque = false;
    // engine.state.set('name', …), and setDefault and setLookup (a variable worked out each time it's read).
    let setter = regex::Regex::new(r"state\.set(?:Default|Lookup)?\(").unwrap();
    for text in &code {
        for found in setter.find_iter(text) {
            match crate::harlowe::args(text[found.end()..].split(')').next().unwrap_or("")).first().and_then(|a| chapbook::literal(a)) {
                Some(name) => {
                    set.insert(name.split('.').next().unwrap_or(&name).to_string());
                }
                // A name worked out as it runs: anything could be set.
                None => opaque = true,
            }
        }
    }

    out.stats.variables = set.len();
    let mut reported: HashSet<String> = HashSet::new();
    for ((_, passage, _), parsed) in ix.passages().zip(&read) {
        out.stats.macros += parsed.inserts.len() + parsed.modifiers.len();
        let mut condition_before = false;
        for (at, modifiers) in &parsed.modifiers {
            for modifier in modifiers {
                let lower = modifier.to_ascii_lowercase();
                if !chapbook::is_modifier(modifier) && !custom.modifiers.iter().any(|r| r.is_match(modifier.trim())) {
                    out.at(Severity::Warning, "unknown-modifier", passage, *at, format!("[{modifier}] isn't a Chapbook modifier"))
                        .about(format!("[{modifier}]"))
                        .hint(props::closest(&lower, &modifier_names).map(|n| format!("did you mean [{n}]?")));
                    continue;
                }
                let word = lower.split_whitespace().next().unwrap_or("");
                if matches!(word, "if" | "ifalways" | "ifnever" | "unless") {
                    condition_before = true;
                } else if word == "else" && !condition_before {
                    out.at(Severity::Error, "else-without-hook", passage, *at, "[else] has no [if] before it in the passage".into())
                        .about("[else]")
                        .hint(Some("put it after an [if] or [unless]".into()));
                }
            }
        }
        for insert in &parsed.inserts {
            if insert.variable {
                let root = insert.name.split('.').next().unwrap_or(&insert.name).to_string();
                if !opaque && !set.contains(&root) && !chapbook::BUILT_IN.contains(&root.as_str()) && reported.insert(root.clone()) {
                    let message = format!("{{{}}} shows a variable that no vars section sets", insert.name);
                    out.at(Severity::Warning, "never-set", passage, insert.start, message)
                        .about(format!("{{{}}}", insert.name))
                        .hint(props::closest(&root, &set.iter().cloned().collect::<Vec<_>>()).map(|n| format!("did you mean {n}?")));
                }
            } else if !chapbook::is_insert(&insert.name) && !custom.inserts.iter().any(|r| r.is_match(insert.name.trim())) {
                // Chapbook shows an insert it doesn't know as it was written.
                let message = format!("{{{}}} isn't a Chapbook {} insert, nor one the story adds, so it shows as written", insert.name, chapbook::VERSION);
                out.at(Severity::Warning, "unknown-macro", passage, insert.start, message).about(format!("{{{}…}}", insert.name)).hint(
                    props::closest(&insert.name.to_ascii_lowercase(), &insert_names.iter().map(|n| n.to_ascii_lowercase()).collect::<Vec<_>>())
                        .map(|n| format!("did you mean {{{n}}}?")),
                );
            }
        }
    }
    let _ = config;
}

/// The variable an input insert sets: `{text input for: 'name'}`, `{cycling link for 'name', …}`.
fn input_variable(insert: &chapbook::Insert) -> Option<String> {
    let name = insert.name.to_ascii_lowercase();
    if !["cycling link", "dropdown menu", "text input"].iter().any(|i| name.starts_with(i)) {
        return None;
    }
    let written = insert.name.split_once(" for ").map(|(_, v)| v.to_string()).or_else(|| insert.value.clone())?;
    let value = chapbook::literal(&written)?;
    Some(value.split('.').next().unwrap_or(&value).to_string())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::checks::test::short;

    fn whole(passages: &str) -> Vec<String> {
        let head = ":: StoryTitle\nT\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"Chapbook\", \"format-version\": \"2.3.1\", \"start\": \"Start\"}\n\n";
        let story = story::Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap();
        let mut found = short(&crate::check(&story, &Config::default()).findings);
        found.sort();
        found
    }

    #[test]
    fn inserts_name_passages_like_links() {
        let found = whole(concat!(
            ":: Start\n{link to: 'Hall'} {link to: 'Nowhere'} {embed passage: 'Doors'} {reveal link: 'Look', passage: 'View'}\n\n",
            ":: Hall\n{back link}\n\n:: Doors\n[[Start]]\n\n:: View\nA view. [[Start]]\n",
        ));
        assert_eq!(found, ["broken-link Start [[Nowhere]]"]);
    }

    #[test]
    fn unknown_inserts_and_modifiers() {
        let found = whole(":: Start\n{bak link} {Back Link}\n[iff x > 1]\nHmm.\n[continue]\n[[Start]]\n");
        assert_eq!(found, ["unknown-macro Start {bak link…}", "unknown-modifier Start [iff x > 1]"]);
    }

    #[test]
    fn the_story_can_add_its_own() {
        let found = whole(concat!(
            ":: Start\n{smiley face} {icon of: 'star'} {frowny face}\n[uppercase]\nLoud.\n[continue]\n[[Start]]\n\n",
            ":: Setup\n[JavaScript]\nengine.extend('2.0.0', () => {\n  engine.template.inserts.add({ match: /^smiley face$/i, render: () => ':)' });\n",
            "  engine.template.inserts.add({ match: /^icon of/i, render: (n) => n });\n",
            "  engine.template.modifiers.add({ match: /^uppercase$/i, process(output) { output.text = output.text.toUpperCase(); } });\n});\n",
        ));
        assert_eq!(found, ["unknown-macro Start {frowny face…}", "unreachable Setup Setup"]);
    }

    #[test]
    fn the_header_and_footer_link_from_every_passage() {
        let found = whole(":: Start\nconfig.footer.center: \"[[Contents]]\"\n--\nA start with no way on.\n\n:: Contents\nAll the parts.\n");
        assert!(found.is_empty(), "the footer's link reaches Contents, and is a way on from both: {found:?}");
    }

    #[test]
    fn else_needs_an_if_before_it() {
        assert!(whole(":: Start\nluck: 2\n--\n[if luck > 1]\nYes.\n[else]\nNo.\n[continue]\n[[Start]]\n").is_empty());
        assert_eq!(whole(":: Start\n[else]\nNo.\n[continue]\n[[Start]]\n"), ["else-without-hook Start [else]"]);
    }

    #[test]
    fn variables_shown_and_never_set() {
        let found = whole(concat!(
            ":: Start\nstrength: 18\n--\n{strength} {strenght} {name} {passage.name} {mood}\n[[Start]]\n\n",
            ":: Script [script]\nengine.state.set('mood', 'calm');\nengine.state.setLookup('name', get => 'Ava');\n",
        ));
        assert_eq!(found, ["never-set Start {strenght}"]);
    }
}
