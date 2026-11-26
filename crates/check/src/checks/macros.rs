//! Macros: unknown, unclosed or misplaced, and widgets nothing calls or code calls as functions.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::config::Config;
use crate::index::{Index, PassageId};
use crate::macros_defined::{MacroKind, StoryMacro};
use crate::out::Out;
use crate::scan::{self};
use crate::*;

/// Where each widget is called as a function: (passage index, offset), by widget name.
type Calls = HashMap<String, Vec<(usize, usize)>>;

/// For the widgets: which the story's code also defines as a function (`function x`, `x =`, `x:`), and where each
/// passage's code first calls one as a function, `x(`, in passage order. All the widgets are found in one pass over
/// the code, not one pass each.
pub(crate) fn widget_calls(ix: &Index, definitions: &[StoryMacro]) -> (HashSet<String>, Calls) {
    let names: Vec<&str> = definitions.iter().filter(|d| d.kind == MacroKind::Widget).map(|d| d.name.as_str()).collect::<BTreeSet<_>>().into_iter().collect();
    let mut functions = HashSet::new();
    let mut calls: Calls = HashMap::new();
    if names.is_empty() {
        return (functions, calls);
    }
    // Pattern 2n is a definition's start for name n, matched with its spellings below; 2n + 1 is a call.
    let mut patterns = vec![];
    for name in &names {
        patterns.push(vec![format!("function {name}"), format!("{name} ="), format!("{name}="), format!("{name}:")]);
    }
    let flat: Vec<(usize, bool, String)> = patterns
        .iter()
        .enumerate()
        .flat_map(|(n, defs)| defs.iter().map(move |d| (n, true, d.clone())))
        .chain(names.iter().enumerate().map(|(n, name)| (n, false, format!("{name}("))))
        .collect();
    let Ok(finder) = aho_corasick::AhoCorasick::new(flat.iter().map(|(_, _, p)| p.as_str())) else {
        return (functions, calls);
    };
    let mut scan = |text: &str, passage: Option<usize>, calls: &mut Calls| {
        let mut first: HashSet<usize> = HashSet::new();
        for found in finder.find_overlapping_iter(text) {
            let (n, definition, _) = &flat[found.pattern().as_usize()];
            if *definition {
                functions.insert(names[*n].to_string());
                continue;
            }
            let Some(index) = passage else { continue };
            // A call: not a method (`.x(`) nor the end of a longer name.
            let before = text[..found.start()].chars().next_back();
            if !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$') && first.insert(*n) {
                calls.entry(names[*n].to_string()).or_default().push((index, found.start()));
            }
        }
    };
    for script in &ix.story.scripts {
        scan(&script.text, None, &mut calls);
    }
    for (index, (_, _passage, parse)) in ix.passages().enumerate() {
        scan(&parse.code, Some(index), &mut calls);
    }
    (functions, calls)
}

pub(crate) fn macros(ix: &Index, config: &Config, out: &mut Out) {
    // The story's own macros over the config's; for a name defined twice, the later definition.
    let mut custom: HashMap<String, bool> = config.macros.clone();
    for defined in ix.macros() {
        custom.insert(defined.name.clone(), defined.container);
    }
    // A custom macro seen closed anywhere is a container.
    let mut closed: HashSet<String> = HashSet::new();
    let tagged: Vec<(PassageId, &[scan::MacroTag])> = ix.passages().map(|(id, _, parse)| (id, parse.tags.as_slice())).collect();
    for (_, tags) in &tagged {
        for tag in tags.iter().filter(|t| t.closing) {
            closed.insert(tag.name.clone());
        }
    }
    let is_container = |name: &str| sugarcube::CONTAINERS.contains(&name) || custom.get(name).is_some_and(|c| *c) || closed.contains(name);
    let known = |name: &str| sugarcube::CONTAINERS.contains(&name) || sugarcube::SINGLES.contains(&name) || custom.contains_key(name);
    // Widgets and JavaScript macros nothing calls: not in a passage, nor in a string of code (Wikifier, <<= "<<x>>">>).
    let mut called: HashSet<String> = tagged.iter().flat_map(|(_, tags)| tags.iter().filter(|t| !t.closing).map(|t| t.name.clone())).collect();
    let mut texts: Vec<&str> = ix.story.scripts.iter().map(|c| c.text.as_str()).collect();
    texts.extend(ix.story.passages.iter().map(|p| p.text.as_str()));
    for text in &texts {
        for (at, _) in text.match_indices("<<") {
            let name: String = text[at + 2..].chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect();
            called.insert(name);
        }
    }
    let (functions, widget_calls) = widget_calls(ix, ix.macros());
    for defined in ix.macros() {
        let name = defined.name.clone();
        // A widget used like a JavaScript function, `<<set _x to name(…)>>`: a widget isn't one, so that throws.
        if defined.kind == MacroKind::Widget && !functions.contains(&name) && !sugarcube::FUNCTIONS.contains(&name.as_str()) {
            for &(index, at) in widget_calls.get(&name).into_iter().flatten() {
                let message = format!("{name}(…) calls the widget <<{name}>> as if it were a JavaScript function; it isn't one, so this throws");
                let finding = out.at(Severity::Error, "widget-as-function", &ix.story.passages[index], at, message);
                finding.subject = Some(format!("{name}(…)"));
                finding.hint = Some(format!("call it as <<{name}>>, or define a JavaScript function in the story's JavaScript"));
            }
        }
        // A widget calls itself in its own definition only if it recurses; its <<widget "x">> tag isn't a call.
        if called.contains(&name) || config.macros.contains_key(&name) {
            continue;
        }
        let message = format!("<<{name}>> is defined, and nothing calls it");
        out.in_file(Severity::Warning, "unused-macro", defined.file.clone(), defined.line, defined.passage.clone(), message).about(format!("<<{name}>>"));
    }
    // [[ and << left open: SugarCube shows them as text, and swallows what follows.
    for (index, tags) in &tagged {
        let (passage, markup) = (ix.passage(*index), &ix.parse(*index).markup);
        let (links, _) = scan::links(markup);
        let mut spans: Vec<(usize, usize)> = links.iter().map(|l| (l.start, l.end)).chain(tags.iter().map(|t| (t.start, t.end))).collect();
        spans.sort_unstable();
        // The furthest any span up to each one reaches: an offset is covered when a span starting at or before it reaches past it.
        let reach: Vec<usize> = spans
            .iter()
            .scan(0, |far, &(_, end)| {
                *far = (*far).max(end);
                Some(*far)
            })
            .collect();
        let covered = |at: usize| {
            let before = spans.partition_point(|&(start, _)| start <= at);
            before > 0 && reach[before - 1] > at
        };
        for (at, _) in markup.match_indices("[[") {
            if !covered(at) && !markup[..at].ends_with('[') {
                let snippet: String = markup[at..].chars().take(30).take_while(|c| *c != '\n').collect();
                out.at(Severity::Error, "unclosed-markup", passage, at, format!("\"{snippet}…\" opens a link with [[ and never closes it with ]]")).subject =
                    Some("[[".into());
            }
        }
        for (at, _) in markup.match_indices("<<") {
            let next = markup[at + 2..].chars().next();
            let looks_like_macro = next.is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '=' || c == '-');
            if looks_like_macro && !covered(at) && !markup[..at].ends_with('<') {
                let name: String = markup[at + 2..].chars().take_while(|c| *c != ' ' && *c != '\n' && *c != '>').take(30).collect();
                out.at(Severity::Error, "unclosed-markup", passage, at, format!("<<{name} is never closed with >>")).subject = Some(format!("<<{name}"));
            }
        }
    }
    for (index, tags) in tagged {
        let passage = ix.passage(index);
        out.stats.macros += tags.len();
        let mut stack: Vec<(String, usize)> = vec![];
        for tag in tags {
            // <<endif>> is the old way to write <</if>>.
            let (name, closing) = match tag.name.strip_prefix("end") {
                Some(rest) if !tag.closing && !known(&tag.name) && is_container(rest) => (rest.to_string(), true),
                _ => (tag.name.clone(), tag.closing),
            };
            if closing {
                match stack.iter().rposition(|(open, _)| *open == name) {
                    Some(at) => {
                        for (open, offset) in stack.drain(at..).skip(1).collect::<Vec<_>>() {
                            out.at(Severity::Error, "unclosed-macro", passage, offset, format!("<<{open}>> isn't closed before <</{name}>>")).subject =
                                Some(format!("<<{open}>>"));
                        }
                    }
                    None => {
                        out.at(Severity::Error, "stray-close", passage, tag.start, format!("<</{name}>> closes nothing: there's no <<{name}>> open here"))
                            .subject = Some(format!("<</{name}>>"));
                    }
                }
                continue;
            }
            if !known(&name) && !closed.contains(&name) {
                let message = format!("<<{name}>> isn't a SugarCube macro, a widget or one the story's JavaScript adds");
                let finding = out.at(Severity::Error, "unknown-macro", passage, tag.start, message);
                finding.subject = Some(format!("<<{name}>>"));
                finding.hint = suggest_macro(&name);
                continue;
            }
            if let Some(parents) = sugarcube::parents_of(&name) {
                let inside = stack.iter().any(|(open, _)| parents.contains(&open.as_str()));
                // <<break>> and <<continue>> can sit in a widget called from a <<for>>.
                if !inside && !(matches!(name.as_str(), "break" | "continue") && passage.has_tag("widget")) {
                    out.at(Severity::Error, "misplaced-macro", passage, tag.start, format!("<<{name}>> belongs inside <<{}>>", parents.join(">> or <<")))
                        .subject = Some(format!("<<{name}>>"));
                }
            }
            if is_container(&name) {
                stack.push((name, tag.start));
            }
        }
        for (open, offset) in stack {
            out.at(Severity::Error, "unclosed-macro", passage, offset, format!("<<{open}>> is never closed")).subject = Some(format!("<<{open}>>"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::test::{run, short, story};

    #[test]
    fn a_macro_defined_in_a_passage_is_known_and_its_unused_twin_is_reported() {
        // Two names from one Macro.add, in a passage's <<script>>: both are known, and the one nothing calls is unused.
        let twee = ":: Start\n<<script>>Macro.add([\"left\", \"right\"], { handler() { Engine.play(\"Start\"); } });<</script>>\n<<right>>\n";
        let findings = short(&run(&story(twee), |ix, out| macros(ix, &Config::default(), out)).0);
        assert_eq!(findings, ["unused-macro Start <<left>>"]);
    }

    #[test]
    fn a_javascript_macro_isnt_a_widget_called_as_a_function() {
        let twee = ":: Start\n<<script>>Macro.add(\"roll\", { handler() {} });<</script>>\n<<roll>><<set _n to roll(6)>>\n";
        let findings = short(&run(&story(twee), |ix, out| macros(ix, &Config::default(), out)).0);
        assert!(findings.is_empty(), "{findings:?}");
    }
}
