//! Links: links to passages that aren't there or can't be shown, and passages the start doesn't reach. Both read the
//! link graph (`graph.rs`).

use std::collections::HashSet;

use crate::config::Config;
use crate::graph::{How, LinkGraph};
use crate::index::Index;
use crate::out::Out;
use crate::*;

/// Why SugarCube won't navigate to (or, for an include, include) a passage: it's code-tagged, or a special passage.
pub(crate) fn code_passage(ix: &Index, name: &str, include: bool) -> Option<String> {
    if !ix.format.sugarcube() {
        return None;
    }
    let passage = ix.passage(ix.named(name)?);
    if let Some(tag) = passage.tags.iter().find(|t| sugarcube::CODE_TAGS.contains(&t.as_str())) {
        let what = if include { "SugarCube can't include" } else { "SugarCube can't go to" };
        return Some(format!("it's tagged {tag}: {what} a code-tagged passage"));
    }
    (!include && sugarcube::CODE_PASSAGES.contains(&name)).then(|| "it's one of SugarCube's special passages, which can't be navigated to".to_string())
}

/// Each link site that goes to no passage, or to one SugarCube won't show; and the counts of links.
pub(crate) fn broken_links(ix: &Index, _config: &Config, out: &mut Out, graph: &LinkGraph) {
    let names = &ix.names.set;
    out.stats.links = graph.sites.len();
    out.stats.dynamic_links = graph.dynamic;
    for site in &graph.sites {
        let (passage, how, target) = (ix.passage(site.passage), site.how.word(), &site.target);
        if names.contains(target.as_str()) {
            // SugarCube won't show a code passage, nor include a code-tagged one.
            if let Some(why) = code_passage(ix, target, site.how == How::Include) {
                let verb = if site.how == How::Include { "includes" } else { "goes to" };
                let message = format!("The {how} {verb} \"{target}\", and {why}");
                out.in_file(Severity::Error, "code-passage", passage.file.clone(), passage.line_at(site.offset), Some(passage.name.clone()), message)
                    .about(target.clone());
            }
        } else if !is_dynamic(target) && !is_external(target) {
            let message = format!("The {how} goes to \"{target}\", and there's no passage by that name");
            out.in_file(Severity::Error, "broken-link", passage.file.clone(), passage.line_at(site.offset), Some(passage.name.clone()), message)
                .about(format!("[[{target}]]"))
                .hint(suggest(target, names));
        }
    }
}

/// A start that names no passage, and the passages nothing leads to from the start. Gives back the passages reached
/// (none, when the story gives nowhere to start from).
pub(crate) fn unreachable(ix: &Index, _config: &Config, out: &mut Out, graph: &LinkGraph) -> HashSet<String> {
    let names = &ix.names.set;
    if let Some(start) = ix.story.start.as_ref().filter(|s| !names.contains(s.as_str())) {
        let (file, line) = ix.story.data_at.clone().unwrap_or_else(|| (ix.story.passages.first().map(|p| p.file.clone()).unwrap_or_default(), 1));
        let message = format!("The story starts at \"{start}\", and there's no passage by that name");
        out.in_file(Severity::Error, "start", file, line, None, message).about(start.clone()).hint(suggest(start, names));
    }
    let Some(reached) = graph.reached(ix) else {
        return HashSet::new();
    };
    out.stats.reached_by_pattern = reached.by_pattern;
    let dynamic = graph.dynamic;
    for (_, passage, _) in ix.passages() {
        if !reached.passages.contains(&passage.name) {
            let hint = (dynamic > 0).then(|| format!("unless through one of the {dynamic} links whose passage is worked out as the story runs"));
            let message = format!("Nothing leads to \"{}\" from the start", passage.name);
            out.in_file(Severity::Warning, "unreachable", passage.file.clone(), passage.line, Some(passage.name.clone()), message)
                .about(passage.name.clone())
                .hint(hint);
        }
    }
    reached.passages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::test::{run, short, story};

    fn reach(story: &story::Story) -> (Vec<String>, LinkGraph, HashSet<String>) {
        let config = Config::default();
        let (findings, _, (graph, reached)) = run(story, |ix, out| {
            let graph = LinkGraph::build(ix);
            broken_links(ix, &config, out, &graph);
            let reached = unreachable(ix, &config, out, &graph);
            (graph, reached)
        });
        (short(&findings), graph, reached)
    }

    #[test]
    fn a_link_to_no_passage_is_broken_and_names_the_closest() {
        let (findings, _, _) = reach(&story(":: Start\n[[Hal]]\n\n:: Hall\nA hall.\n"));
        assert!(findings.contains(&"broken-link Start [[Hal]]".to_string()), "{findings:?}");
        let (all, _, _) = run(&story(":: Start\n[[Hal]]\n\n:: Hall\nA hall.\n"), |ix, out| broken_links(ix, &Config::default(), out, &LinkGraph::build(ix)));
        assert_eq!(all[0].hint.as_deref(), Some("did you mean \"Hall\"?"));
    }

    #[test]
    fn a_link_to_a_widget_passage_goes_nowhere() {
        let (findings, _, _) = reach(&story(":: Start\n[[Tools]]\n\n:: Tools [widget]\n<<widget \"hammer\">>Bang.<</widget>>\n"));
        assert!(findings.contains(&"code-passage Start Tools".to_string()), "{findings:?}");
    }

    #[test]
    fn passages_are_reached_by_links_strings_and_names_built_with_plus() {
        let (findings, graph, reached) = reach(&story(
            ":: Start\n[[Hall]] <<set $next to \"Cellar\">><<goto \"Day\" + $n>><<link \"On\" \"Night\"+$n>><</link>>\n\n:: Hall\nA hall.\n\n:: Cellar\nDark.\n\n:: Day1\nOne.\n\n:: Day2\nTwo.\n\n:: Night1\nDark.\n\n:: Attic\nDusty.\n",
        ));
        assert_eq!(findings, ["unreachable Attic Attic"]);
        for name in ["Start", "Hall", "Cellar", "Day1", "Day2", "Night1"] {
            assert!(reached.contains(name), "{name} isn't reached: {reached:?}");
        }
        assert!(graph.edges["Start"].contains("Hall"));
    }

    #[test]
    fn a_name_built_from_a_string_is_dynamic_and_a_plain_one_still_breaks() {
        let (findings, graph, _) = reach(&story(":: Start\n<<goto \"Day\" + $n>>\n<<goto \"Nowhere\">>\n\n:: Day1\nOne.\n"));
        assert_eq!(findings, ["broken-link Start [[Nowhere]]"]);
        assert_eq!(graph.dynamic, 1);
        assert_eq!(graph.sites[0].target, "\"Day\" + $n");
    }

    #[test]
    fn a_link_to_a_variable_counts_as_dynamic() {
        let (_, graph, _) = reach(&story(":: Start\n[[Go|$where]]\n"));
        assert_eq!(graph.dynamic, 1);
    }
}
