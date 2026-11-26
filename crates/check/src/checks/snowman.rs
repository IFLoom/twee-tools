//! Snowman 2's own rule: `<% … %>` templates left open, which swallow the rest of the passage. The passages its calls
//! name (`story.show('Hall')`, `story.render('Map')`) are checked as links by the graph.

use crate::Severity;
use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::snowman;

pub(crate) fn snowman(ix: &Index, _config: &Config, out: &mut Out) {
    for (_, passage, _) in ix.passages() {
        for at in snowman::unclosed_templates(&passage.text) {
            out.at(Severity::Error, "unclosed-markup", passage, at, "A template opened with <% is never closed with %>".into())
                .about("<%")
                .hint(Some("close it with %>".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::checks::test::short;
    use crate::config::Config;

    fn whole(version: &str, passages: &str) -> Vec<String> {
        let head = format!(
            ":: StoryTitle\nT\n\n:: StoryData\n{{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"Snowman\", \"format-version\": \"{version}\", \"start\": \"Start\"}}\n\n"
        );
        let story = story::Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap();
        let mut found = short(&crate::check(&story, &Config::default()).findings);
        found.sort();
        found
    }

    #[test]
    fn snowman_2_calls_name_passages() {
        let found = whole(
            "2.1.4",
            ":: Start\n<% if (hasVisited('Hal')) { story.show('Hall') } %>[[Den]]\n\n:: Hall\n<%= story.render('Map') %>\n\n:: Map\nA map. [[Start]]\n\n:: Den\n<% story.show('Cellar') %>\n",
        );
        assert_eq!(
            found,
            ["broken-link Start [[Hal]]", "broken-link Den [[Cellar]]"]
                .map(String::from)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn templates_left_open_and_dead_ends() {
        let found = whole("2.1.4", ":: Start\n[[Hall]] [[Den]]\n\n:: Hall\n<% s.x = 1 \n\n:: Den\nA den with no way on.\n");
        assert_eq!(found, ["dead-end Den Den", "dead-end Hall Hall", "unclosed-markup Hall <%"]);
    }
}
