//! Harlowe's own rules: macros it doesn't have, macro calls and hooks left open, `(else:)` with nothing to be the other
//! way of, and deprecated macros. Links and passages its macros name are checked with every format's, by the graph.

use crate::config::Config;
use crate::harlowe::{self, Call};
use crate::index::Index;
use crate::out::Out;
use crate::{Severity, props};

pub(crate) fn harlowe(ix: &Index, _config: &Config, out: &mut Out) {
    let names = harlowe::all_names();
    let added = harlowe::script_macros(ix.story.scripts.iter().map(|c| c.text.as_str()));
    for (_, passage, parse) in ix.passages() {
        let calls = &parse.calls;
        out.stats.macros += calls.len();
        for call in calls {
            let shown = format!("({}:)", call.name);
            if !call.closed {
                out.at(Severity::Error, "unclosed-markup", passage, call.start, format!("({}: is never closed with )", call.name)).about(shown.clone());
                continue;
            }
            // A custom macro is called through the variable holding it: whether that's set is the variables check's.
            if call.is_custom() {
                continue;
            }
            if harlowe::lookup(&call.name).is_none() && added.contains(&harlowe::normalise(&call.name)) {
                continue;
            }
            let Some(def) = harlowe::lookup(&call.name) else {
                out.at(Severity::Error, "unknown-macro", passage, call.start, format!("{shown} isn't a Harlowe macro"))
                    .about(shown.clone())
                    .hint(props::closest(&call.name.to_ascii_lowercase(), &names).map(|n| format!("did you mean ({n}:)?")));
                continue;
            };
            if def.category == "deprecated" {
                out.at(Severity::Note, "deprecated", passage, call.start, format!("{shown} is deprecated in Harlowe {}", harlowe::table::VERSION))
                    .about(shown.clone());
            }
            if matches!(def.name, "else" | "else-if") && !hook_before(&parse.markup, calls, call) {
                let message = format!("{shown} has no hook before it to be the other way of");
                out.at(Severity::Error, "else-without-hook", passage, call.start, message)
                    .about(shown.clone())
                    .hint(Some("put it right after an (if:), (unless:) or (else-if:) and its hook".into()));
            }
        }
        for at in harlowe::unclosed_hooks(&parse.markup, calls) {
            out.at(Severity::Error, "unclosed-markup", passage, at, "A hook opened with [ is never closed with ]".into())
                .about("[")
                .hint(Some("close it with ], or write [= to run it to the end of the passage on purpose".into()));
        }
    }
}

/// Whether some call before this one has a hook attached: what `(else:)` is the other way of.
fn hook_before(markup: &str, calls: &[Call], this: &Call) -> bool {
    // Harlowe attaches a hook across whitespace and line breaks, as it finds the next element after the changer.
    calls.iter().any(|c| c.closed && c.end <= this.start && markup[c.end..].trim_start().starts_with('['))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::checks::test::{run, short};

    fn findings(passages: &str) -> Vec<String> {
        let head = ":: StoryTitle\nT\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"Harlowe\", \"format-version\": \"3.3.9\", \"start\": \"Start\"}\n\n";
        let story = story::Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap();
        short(&run(&story, |ix, out| harlowe(ix, &Config::default(), out)).0)
    }

    #[test]
    fn unknown_macros_written_any_way_harlowe_accepts() {
        assert!(findings(":: Start\n(Go_To: \"Start\") (link-replace: \"x\")[y] (goto: \"Start\")\n").is_empty());
        assert_eq!(findings(":: Start\n(lnk-goto: \"x\", \"Start\")\n"), ["unknown-macro Start (lnk-goto:)"]);
    }

    #[test]
    fn macros_an_add_on_adds_in_javascript() {
        let passages = ":: Start\n(track: 'bell', 'play')(newtrack: 'bell', 'bell.mp3')(trak: 'bell', 'play')\n\n:: Lib [script]\nwindow.Chapel.Macros.add({newtrack:function(){return 1},track:function(e,t){return t}});\n";
        assert_eq!(findings(passages), ["unknown-macro Start (trak:)"]);
    }

    #[test]
    fn custom_macros_are_called_through_variables() {
        assert!(findings(":: Start\n(set: $greet to (macro: str-type _n, [(output: \"Hi \" + _n)]))($greet: \"Ava\")\n").is_empty());
    }

    #[test]
    fn calls_and_hooks_left_open() {
        assert_eq!(findings(":: Start\n(print: \"never closed\"\n"), ["unclosed-markup Start (print:)"]);
        assert_eq!(findings(":: Start\n(if: $a)[Rich.\n"), ["unclosed-markup Start ["]);
        assert!(findings(":: Start\n(if: $a)[=Rich to the end.\n").is_empty(), "[= runs to the end on purpose");
    }

    #[test]
    fn else_needs_a_hook_before_it() {
        assert!(findings(":: Start\n(if: $a)[Yes](else:)[No]\n").is_empty());
        assert_eq!(findings(":: Start\nPlain text. (else:)[No]\n"), ["else-without-hook Start (else:)"]);
    }

    #[test]
    fn deprecated_macros_are_a_note() {
        assert_eq!(findings(":: Start\n|x>[Hi](mouseover: ?x)[Boo]\n"), ["deprecated Start (mouseover:)"]);
    }

    /// The whole check, for what the link graph and dead ends do with Harlowe's macros.
    fn whole(passages: &str) -> Vec<String> {
        let head = ":: StoryTitle\nT\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"Harlowe\", \"format-version\": \"3.3.9\", \"start\": \"Start\"}\n\n";
        let story = story::Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap();
        short(&crate::check(&story, &Config::default()).findings)
    }

    #[test]
    fn its_macros_link_and_break_like_links() {
        let found = whole(concat!(
            ":: Start\n(link-goto: \"Hall\")(link-goto: \"Go on\", \"Garden\")(go-to: \"Nowehre\")(click-goto: \"x\", \"Mapp\")\n\n",
            ":: Hall\n(go-to: $next)[[Garden]]\n\n:: Garden\n[[The End]]\n\n:: The End\nFin.\n",
        ));
        let broken: Vec<&String> = found.iter().filter(|f| f.starts_with("broken-link")).collect();
        assert_eq!(broken, ["broken-link Start [[Nowehre]]", "broken-link Start [[Mapp]]"], "{found:?}");
        assert!(!found.iter().any(|f| f.starts_with("unreachable")), "{found:?}");
    }

    #[test]
    fn passages_chosen_by_their_tags_are_reached() {
        let found =
            whole(":: Start\n(redirect: (passages: where its tags contains \"first\")'s 1st's name)\n\n:: Hall [first]\n[[The End]]\n\n:: The End\nFin.\n");
        assert!(!found.iter().any(|f| f.starts_with("unreachable")), "{found:?}");
    }

    #[test]
    fn dead_ends_know_harlowe_ways_on() {
        let found = whole(concat!(
            ":: Start\n[[Hall]] [[Den]] [[Stuck]]\n\n:: Hall\n(link: \"Leave\")[(go-to: \"Start\")]\n\n",
            ":: Den\n(display: \"Doors\")\n\n:: Doors\n[[Start]]\n\n:: Stuck\nNothing here.\n",
        ));
        let dead: Vec<&String> = found.iter().filter(|f| f.starts_with("dead-end")).collect();
        assert_eq!(dead, ["dead-end Stuck Stuck"], "{found:?}");
        // A footer with a link is a way on from every passage.
        let framed = whole(":: Start\n[[Stuck]]\n\n:: Stuck\nNothing here.\n\n:: Menu [footer]\n(link-goto: \"Home\", \"Start\")\n");
        assert!(!framed.iter().any(|f| f.starts_with("dead-end")), "{framed:?}");
    }
}
