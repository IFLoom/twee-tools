//! The story's format, decided once per check: whose rules apply, which passages and tags it runs or loads without a
//! link, and which of its macros name a passage. A check asks this; none tests the format's name itself.

use story::{Passage, Story};

use crate::sugarcube;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    /// SugarCube, or a story that names no format, which `twee-tools build` builds with SugarCube.
    SugarCube,
    Harlowe,
    Chapbook,
    Snowman,
    /// Any other format: only the checks every story shares.
    Other,
}

/// What Twine keeps in passages for itself, in every format: the story's data, and media passages.
const TWINE_PASSAGES: &[&str] = &["StoryTitle", "StoryData"];
pub(crate) const TWINE_TAGS: &[&str] = &["Twine.image", "Twine.audio", "Twine.video", "Twine.vtt"];

/// Harlowe runs these passages itself: at the start, and before or after every passage.
const HARLOWE_TAGS: &[&str] = &["startup", "header", "footer", "debug-startup", "debug-header", "debug-footer"];

impl Format {
    pub(crate) fn of(story: &Story) -> Format {
        match story.format.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None | Some("sugarcube") => Format::SugarCube,
            Some("harlowe") => Format::Harlowe,
            Some("chapbook") => Format::Chapbook,
            Some("snowman") => Format::Snowman,
            Some(_) => Format::Other,
        }
    }

    /// Whether SugarCube's macros, names and rules apply.
    pub(crate) fn sugarcube(self) -> bool {
        self == Format::SugarCube
    }

    /// Passages the format runs or shows by name, so nothing needs to link to them.
    pub(crate) fn special_passages(self) -> &'static [&'static str] {
        match self {
            Format::SugarCube => sugarcube::SPECIAL_PASSAGES,
            _ => &[],
        }
    }

    /// Tags that make the format run or load a passage without a link.
    pub(crate) fn special_tags(self) -> &'static [&'static str] {
        match self {
            Format::SugarCube => sugarcube::SPECIAL_TAGS,
            Format::Harlowe => HARLOWE_TAGS,
            _ => &[],
        }
    }

    /// A passage the story reaches without a link: one the format runs by name or by tag, or one Twine keeps for itself.
    pub(crate) fn is_special(self, passage: &Passage) -> bool {
        self.is_special_name(&passage.name) || passage.tags.iter().any(|t| TWINE_TAGS.contains(&t.as_str()) || self.special_tags().contains(&t.as_str()))
    }

    pub(crate) fn is_special_name(self, name: &str) -> bool {
        TWINE_PASSAGES.contains(&name) || self.special_passages().contains(&name)
    }

    /// Which arguments of a macro name a passage to go to or include (`<<goto "Hall">>`: the first).
    pub(crate) fn passage_args(self, name: &str, count: usize) -> &'static [usize] {
        match self {
            Format::SugarCube => sugarcube::passage_args(name, count),
            _ => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    use super::*;
    use crate::checks::test::{run, short};
    use crate::config::Config;
    use crate::graph::LinkGraph;

    /// A story in a format (or none), with these passages after a Start that links nowhere.
    fn story(format: Option<&str>, passages: &str) -> Story {
        let format = format.map(|f| format!(", \"format\": \"{f}\"")).unwrap_or_default();
        let head = format!(
            ":: StoryTitle\nT\n\n:: StoryData\n{{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\"{format}, \"start\": \"Start\"}}\n\n:: Start\nHi.\n\n"
        );
        Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap()
    }

    fn unreachable(story: &Story) -> Vec<String> {
        short(&run(story, |ix, out| crate::checks::links::unreachable(ix, &Config::default(), out, &LinkGraph::build(ix))).0)
    }

    #[test]
    fn decided_once_from_the_story() {
        let of = |f: Option<&str>| Format::of(&story(f, ""));
        assert_eq!(of(None), Format::SugarCube, "a story that names no format builds as SugarCube");
        assert_eq!(of(Some("SugarCube")), Format::SugarCube);
        assert_eq!(of(Some("Harlowe")), Format::Harlowe);
        assert_eq!(of(Some("chapbook")), Format::Chapbook);
        assert_eq!(of(Some("Paperthin")), Format::Other);
    }

    #[test]
    fn each_format_runs_its_own_passages() {
        // Harlowe runs startup and header passages itself; SugarCube runs StoryInit.
        let passages = ":: Setup [startup]\nx\n\n:: Menu [header]\nx\n\n:: StoryInit\nx\n\n:: Picture [Twine.image]\ndata:image/png;base64,AA==\n";
        assert_eq!(unreachable(&story(Some("Harlowe"), passages)), ["unreachable StoryInit StoryInit"]);
        let mut sugarcube = unreachable(&story(Some("SugarCube"), passages));
        sugarcube.sort();
        assert_eq!(sugarcube, ["unreachable Menu Menu", "unreachable Setup Setup"], "startup and header mean nothing to SugarCube");
        assert_eq!(unreachable(&story(Some("Chapbook"), passages)).len(), 3, "only Twine's own media passage is special everywhere");
    }

    #[test]
    fn a_story_with_no_format_reads_macros_as_sugarcube_does() {
        let graph = |story: &Story| {
            let ix = crate::index::Index::new(story);
            LinkGraph::build(&ix).sites.iter().map(|s| s.target.clone()).collect::<HashSet<String>>()
        };
        let passages = ":: Hall\n<<goto \"Cellar\">>\n";
        assert!(graph(&story(None, passages)).contains("Cellar"));
        assert!(!graph(&story(Some("Harlowe"), passages)).contains("Cellar"), "<<goto>> is no Harlowe macro");
    }
}
