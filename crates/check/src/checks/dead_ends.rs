//! Passages the player reaches and can't leave.

use std::collections::{HashMap, HashSet};

use crate::config::Config;
use crate::format::Format;
use crate::graph::LinkGraph;
use crate::index::{Index, named};
use crate::out::Out;
use crate::*;
use crate::{chapbook, harlowe, snowman};

/// Passages a player reaches and can't leave: no link, no way forward. Not checked when the header, footer, sidebar or
/// menu has links of its own, as every passage then has a way out; and not for endings, nor for passages only included.
pub(crate) fn dead_ends(ix: &Index, _config: &Config, out: &mut Out, graph: &LinkGraph, reached: &HashSet<String>) {
    // The widgets and JavaScript macros that can move the player: their own way out.
    let bodies: Vec<(&String, &String)> = ix.macros().iter().map(|m| (&m.name, &m.body)).collect();
    let mut navigating: HashSet<String> = HashSet::new();
    for &(name, body) in &bodies {
        if has_way_out(body) {
            navigating.insert(name.clone());
        }
    }
    // A widget that calls a navigating one navigates too.
    loop {
        let before = navigating.len();
        for &(name, body) in &bodies {
            if !navigating.contains(name) && calls_any(body, &navigating) {
                navigating.insert(name.clone());
            }
        }
        if navigating.len() == before {
            break;
        }
    }
    let mut exits: HashMap<&str, bool> = HashMap::new();
    for (_, passage, parse) in ix.passages() {
        let parsed = parse;
        let html_link = || ["<a ", "data-passage", "<tw-link"].iter().any(|m| parsed.markup.to_ascii_lowercase().contains(m));
        let mut own = match ix.format {
            Format::Harlowe => harlowe_way_out(&parsed.markup, &parsed.calls),
            Format::Chapbook => chapbook_way_out(&passage.text, &parsed.markup) || html_link(),
            Format::Snowman => !snowman::links(&parsed.markup).is_empty() || snowman::goes_on(&parsed.code) || html_link(),
            _ => {
                way_out(&passage.text, &parsed.markup, &parsed.tags)
                    || (!navigating.is_empty() && parsed.tags.iter().any(|t| !t.closing && navigating.contains(&t.name)))
            }
        };
        if !own && !named(&parsed.code, &ix.names).is_empty() {
            own = true;
        }
        exits.insert(&passage.name, own);
    }
    let included: HashSet<&String> = graph.includes.values().flatten().collect();
    // Including a passage with a way out gives one.
    loop {
        let mut changed = false;
        for (name, targets) in &graph.includes {
            if !exits[name.as_str()] && targets.iter().any(|t| exits.get(t.as_str()).copied().unwrap_or(false)) {
                exits.insert(name, true);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Shown with every passage: SugarCube's header, footer and menus; Harlowe's header and footer passages.
    let frame = ["PassageHeader", "PassageFooter", "StoryCaption", "StoryMenu", "StoryBanner", "PassageDone", "StoryInterface", "PassageReady"];
    // Chapbook's header and footer, set in a vars section: a way on there is a way on from every passage.
    let chapbook_frame =
        ix.format == Format::Chapbook && ix.passages().any(|(_, p, _)| chapbook::read(&p.text).frame.iter().any(|(_, text)| chapbook_way_out(text, text)));
    let framed = chapbook_frame
        || ix.passages().any(|(_, p, _)| {
            let shown_with_every = match ix.format {
                Format::Harlowe => p.tags.iter().any(|t| matches!(t.as_str(), "header" | "footer" | "debug-header" | "debug-footer")),
                _ => frame.contains(&p.name.as_str()),
            };
            shown_with_every && exits.get(p.name.as_str()).copied().unwrap_or(false)
        });
    if framed {
        return;
    }
    for (_, passage, _parse) in ix.passages() {
        let special = ix.format.is_special(passage);
        let skip = exits[passage.name.as_str()] || special || !reached.contains(&passage.name) || included.contains(&passage.name) || is_ending(passage);
        if skip || is_unwritten(passage) {
            continue;
        }
        let message = format!("\"{}\" has no link or other way forward", passage.name);
        out.in_file(Severity::Warning, "dead-end", passage.file.clone(), passage.line, Some(passage.name.clone()), message).about(passage.name.clone());
    }
}

/// A Harlowe passage's own way on: a link, a macro that moves the player, a custom macro (which might), or an HTML link.
fn harlowe_way_out(markup: &str, calls: &[harlowe::Call]) -> bool {
    !harlowe::links(markup).is_empty()
        || calls.iter().any(|c| c.is_custom() || harlowe::navigates(&c.name))
        || ["<a ", "data-passage", "<tw-link"].iter().any(|m| markup.to_ascii_lowercase().contains(m))
}

/// A Chapbook passage's own way on: a link, an insert that is one, or JavaScript (which might move the player).
fn chapbook_way_out(text: &str, markup: &str) -> bool {
    let read = chapbook::read(text);
    !chapbook::links(markup).is_empty()
        || read.inserts.iter().any(|i| {
            let name = i.name.to_ascii_lowercase();
            ["link to", "back link", "restart link"].iter().any(|l| name.starts_with(l))
        })
        || read.modifiers.iter().any(|(_, m)| m.iter().any(|m| m.eq_ignore_ascii_case("javascript")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::test::{run, short, story};

    /// The dead ends in a story whose every passage is reached.
    fn dead_ends_in(twee: &str) -> Vec<String> {
        let story = story(twee);
        let reached: HashSet<String> = story.passages.iter().map(|p| p.name.clone()).collect();
        short(&run(&story, |ix, out| dead_ends(ix, &Config::default(), out, &LinkGraph::build(ix), &reached)).0)
    }

    #[test]
    fn a_passage_with_no_way_on_is_a_dead_end() {
        assert_eq!(dead_ends_in(":: Start\n[[Hall]]\n\n:: Hall\nA hall, and nothing else.\n"), ["dead-end Hall Hall"]);
    }

    #[test]
    fn including_a_passage_with_a_link_is_a_way_on() {
        assert!(dead_ends_in(":: Start\n[[Hall]]\n\n:: Hall\n<<include \"Doors\">>\n\n:: Doors\n[[Start]]\n").is_empty());
    }

    #[test]
    fn a_widget_that_moves_the_player_is_a_way_on() {
        let twee = ":: Start\n[[Hall]]\n\n:: Hall\n<<leave>>\n\n:: Tools [widget]\n<<widget \"leave\">><<goto \"Start\">><</widget>>\n";
        assert!(dead_ends_in(twee).is_empty(), "{:?}", dead_ends_in(twee));
    }

    #[test]
    fn an_ending_isnt_a_dead_end() {
        assert!(dead_ends_in(":: Start\n[[The End]]\n\n:: The End\nThanks for playing.\n").is_empty());
    }
}

#[test]
fn every_name_of_a_navigating_javascript_macro_is_a_way_on() {
    let twee = ":: Start\n[[Hall]]\n\n:: Setup [script]\nMacro.add([\"back\", \"leave\"], { handler() { Engine.play(\"Start\"); } });\n\n:: Hall\n<<leave>>\n";
    let story = crate::checks::test::story(twee);
    let reached: HashSet<String> = story.passages.iter().map(|p| p.name.clone()).collect();
    let findings = crate::checks::test::run(&story, |ix, out| dead_ends(ix, &Config::default(), out, &LinkGraph::build(ix), &reached)).0;
    assert!(findings.is_empty(), "{:?}", crate::checks::test::short(&findings));
}
