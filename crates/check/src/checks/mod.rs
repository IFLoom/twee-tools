//! The checks, one module per stage of `check_with`. Each reads the `Index` and reports into an `Out`.

pub(crate) mod chapbook;
pub(crate) mod dead_ends;
pub(crate) mod harlowe;
pub(crate) mod javascript;
pub(crate) mod links;
pub(crate) mod macros;
pub(crate) mod media;
pub(crate) mod names;
pub(crate) mod pitfalls;
pub(crate) mod snowman;
pub(crate) mod story_data;
pub(crate) mod structure;
pub(crate) mod variables;

/// For the checks' tests: a SugarCube story from Twee passages, its index, and what a check reports on it.
#[cfg(test)]
pub(crate) mod test {
    use std::path::Path;

    use story::Story;

    use crate::index::Index;
    use crate::out::Out;
    use crate::{Finding, Stats};

    pub(crate) fn story(passages: &str) -> Story {
        let head = ":: StoryTitle\nTest\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"SugarCube\", \"format-version\": \"2.37.3\", \"start\": \"Start\"}\n\n";
        Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap()
    }

    /// Runs `check` on the story, and gives back its findings and counts with what it returned.
    pub(crate) fn run<T>(story: &Story, check: impl FnOnce(&Index, &mut Out) -> T) -> (Vec<Finding>, Stats, T) {
        let ix = Index::new(story);
        let mut out = Out::default();
        let value = check(&ix, &mut out);
        let (findings, stats, _) = out.into_parts();
        (findings, stats, value)
    }

    /// Each finding as "code passage subject", to compare in a line.
    pub(crate) fn short(findings: &[Finding]) -> Vec<String> {
        findings.iter().map(|f| format!("{} {} {}", f.code, f.passage.as_deref().unwrap_or("-"), f.subject.as_deref().unwrap_or("-"))).collect()
    }
}
