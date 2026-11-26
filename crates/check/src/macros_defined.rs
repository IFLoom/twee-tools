//! The macros a story defines itself: its widgets, and `Macro.add` in its JavaScript (the story's own, and in passages).
//! Found once, when the index is built; every check that asks "does the story define <<x>>?" reads this list.

use std::path::PathBuf;

use story::Story;

use crate::index::PassageParse;
use crate::scan::{self, Arg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacroKind {
    /// `<<widget "name">>` in a passage tagged widget.
    Widget,
    /// `Macro.add("name", …)` in JavaScript.
    Script,
}

#[derive(Debug, Clone)]
pub(crate) struct StoryMacro {
    pub(crate) name: String,
    pub(crate) kind: MacroKind,
    pub(crate) file: PathBuf,
    pub(crate) line: usize,
    /// The passage it's defined in, if it's in one.
    pub(crate) passage: Option<String>,
    /// Whether it takes a body and a closing tag: `<<widget "x" container>>`, or `tags` in its definition.
    pub(crate) container: bool,
    /// Its definition: a widget's body, or a JavaScript macro's code up to the next `Macro.add` (at most `BODY` bytes).
    pub(crate) body: String,
}

/// How much of a JavaScript macro's definition is kept, when nothing ends it sooner.
const BODY: usize = 6000;

/// Every macro the story defines, in a fixed order: widgets, then `Macro.add` in the story's JavaScript, then in
/// passages. A name defined twice is listed twice; where a check keeps one, the later wins.
pub(crate) fn find(story: &Story, parses: &[PassageParse]) -> Vec<StoryMacro> {
    let mut out = vec![];
    for (passage, parse) in story.passages.iter().zip(parses).filter(|(p, _)| p.has_tag("widget")) {
        let tags = &parse.tags;
        for (at, tag) in tags.iter().enumerate().filter(|(_, t)| t.name == "widget" && !t.closing) {
            let args = scan::args(&tag.args);
            let Some(Arg::Str(name) | Arg::Bare(name)) = args.first() else { continue };
            let end = tags[at..].iter().find(|t| t.name == "widget" && t.closing).map(|t| t.start).unwrap_or(passage.text.len());
            out.push(StoryMacro {
                name: name.clone(),
                kind: MacroKind::Widget,
                file: passage.file.clone(),
                line: passage.line_at(tag.start),
                passage: Some(passage.name.clone()),
                container: args.iter().skip(1).any(|a| matches!(a, Arg::Bare(w) | Arg::Str(w) if w == "container")),
                body: passage.text[tag.end.min(end)..end].to_string(),
            });
        }
    }
    for code in &story.scripts {
        let line = |at: usize| code.line + code.text[..at].matches('\n').count();
        added(&code.text, &code.file, &line, None, &mut out);
    }
    for passage in &story.passages {
        let line = |at: usize| passage.line_at(at);
        added(&passage.text, &passage.file, &line, Some(&passage.name), &mut out);
    }
    out
}

/// Each `Macro.add(` in some text: its names (one string, or an array of them), its definition and whether it's a container.
fn added(text: &str, file: &std::path::Path, line: &dyn Fn(usize) -> usize, passage: Option<&str>, out: &mut Vec<StoryMacro>) {
    for (at, _) in text.match_indices("Macro.add(") {
        let rest = &text[at + "Macro.add(".len()..];
        let head = rest.trim_start();
        let names: Vec<String> = if head.starts_with('[') {
            scan::strings(&head[..head.find(']').unwrap_or(head.len())]).into_iter().map(|s| s.1).collect()
        } else {
            // Only a name written as a string: `Macro.add(name, …)` names it elsewhere.
            scan::strings(head).into_iter().take(1).filter(|(o, _, _)| *o == 1).map(|s| s.1).collect()
        };
        let mut end = rest.find("Macro.add(").unwrap_or(rest.len()).min(BODY);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let body = &rest[..end];
        let container = body.contains("tags:") || body.contains("tags :") || body.contains("\"tags\"");
        for name in names {
            out.push(StoryMacro {
                name,
                kind: MacroKind::Script,
                file: file.to_path_buf(),
                line: line(at),
                passage: passage.map(str::to_string),
                container,
                body: body.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::test::story;
    use crate::index::Index;

    fn macros(twee: &str) -> Vec<StoryMacro> {
        let story = story(twee);
        Index::new(&story).macros().to_vec()
    }

    #[test]
    fn widgets_and_javascript_macros_in_scripts_and_passages() {
        let found = macros(concat!(
            ":: Tools [widget]\n<<widget \"hammer\">>Bang.<</widget>>\n<<widget \"box\" container>>_contents<</widget>>\n\n",
            ":: Setup [script]\nMacro.add(\"say\", { handler() {} });\n\n",
            ":: Start\n<<script>>Macro.add([\"left\", \"right\"], { tags: [\"else\"], handler() {} });<</script>>\n",
        ));
        let summary: Vec<(&str, MacroKind, bool, Option<&str>)> = found.iter().map(|m| (m.name.as_str(), m.kind, m.container, m.passage.as_deref())).collect();
        assert_eq!(
            summary,
            [
                ("hammer", MacroKind::Widget, false, Some("Tools")),
                ("box", MacroKind::Widget, true, Some("Tools")),
                ("say", MacroKind::Script, false, None),
                ("left", MacroKind::Script, true, Some("Start")),
                ("right", MacroKind::Script, true, Some("Start")),
            ]
        );
        assert_eq!(found[0].body, "Bang.");
    }

    #[test]
    fn a_name_kept_in_a_variable_isnt_guessed() {
        assert!(macros(":: Setup [script]\nMacro.add(name, { handler() { this.output.append(\"hi\"); } });\n").is_empty());
    }

    #[test]
    fn a_long_definition_is_cut_on_a_character() {
        let long = "é".repeat(BODY);
        let found = macros(&format!(":: Setup [script]\nMacro.add(\"x\", {{ handler() {{ /* {long} */ }} }});\n"));
        assert!(found[0].body.len() <= BODY && found[0].body.is_char_boundary(found[0].body.len()));
    }
}
