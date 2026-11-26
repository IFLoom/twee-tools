//! A story and a format to a published page: the format's `source` with `{{STORY_NAME}}` and `{{STORY_DATA}}` filled in,
//! `<tw-storydata>` as Twine writes it.

use story::{Passage, Story};

use crate::formats::Format;

/// How to compile.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The passage to start at, over the story's own.
    pub start: Option<String>,
    /// Test mode: `options="debug"`, which SugarCube reads as its debug views.
    pub test: bool,
    /// HTML put before `</head>`: the head file and modules.
    pub head: Vec<String>,
}

/// What compiling made, and what it noticed on the way.
#[derive(Debug, Clone)]
pub struct Page {
    pub html: String,
    pub passages: usize,
    pub notes: Vec<String>,
}

/// The five characters HTML needs escaped, in text and in attribute values alike.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Code put raw into `<script>` or `<style>`, with its own closing tag broken so it can't end the element early.
fn raw_code(text: &str, element: &str) -> String {
    let closing = format!("</{element}");
    let mut out = String::with_capacity(text.len());
    let lower = text.to_ascii_lowercase();
    let mut last = 0;
    for (at, _) in lower.match_indices(&closing) {
        out.push_str(&text[last..at]);
        out.push_str("<\\/");
        out.push_str(&text[at + 2..at + closing.len()]);
        last = at + closing.len();
    }
    out.push_str(&text[last..]);
    out
}

/// The story's scripts or stylesheets as one: one as it is; several each under a `/* twine-user-script #N: "name" */` line.
fn joined(parts: &[(String, String)], kind: &str) -> String {
    match parts {
        [] => String::new(),
        [(_, text)] => text.clone(),
        many => {
            let mut out = String::new();
            for (at, (name, text)) in many.iter().enumerate() {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&format!("/* twine-user-{kind} #{}: \"{}\" */\n", at + 1, name.replace("*/", "* /")));
                out.push_str(text);
            }
            out
        }
    }
}

/// Where Twine's map shows a passage with no position: ten to a row, 125 apart.
pub fn default_position(pid: usize) -> String {
    let (mut x, mut y) = (pid % 10, pid / 10);
    if x == 0 {
        x = 10;
    } else {
        y += 1;
    }
    format!("{},{}", x * 125 - 25, y * 125 - 25)
}

/// The story's `<tw-storydata>`, with the format it's for: a Twine archive holds just this.
pub fn archive(story: &Story, format: &Format, options: &Options) -> Result<String, String> {
    let page =
        compile(&Story { ..story.clone() }, &Format { source: "{{STORY_DATA}}".into(), ..format.clone() }, &Options { head: vec![], ..options.clone() })?;
    Ok(format!("{}\n", page.html))
}

/// The story as Twine 2's JSON: name, IFID, format, start and passages.
pub fn json(story: &Story, format: &Format, options: &Options) -> Result<String, String> {
    let name = story.title.clone().ok_or("The story has no title: add a StoryTitle passage")?;
    let passages: Vec<serde_json::Value> = story
        .passages
        .iter()
        .filter(|p| !matches!(p.name.as_str(), "StoryTitle" | "StoryData") && !p.has_tag("Twine.private"))
        .map(|p| serde_json::json!({ "name": p.name, "tags": p.tags, "text": p.text.trim() }))
        .collect();
    let value = serde_json::json!({
        "name": name,
        "ifid": story.ifid,
        "format": format.name,
        "format-version": format.version,
        "start": options.start.clone().or(story.start.clone()).unwrap_or_else(|| "Start".into()),
        "tag-colors": story.tag_colors,
        "zoom": story.zoom.unwrap_or(1.0),
        "creator": "twee-tools",
        "creator-version": env!("CARGO_PKG_VERSION"),
        "options": if options.test { "debug" } else { "" },
        "script": story.scripts.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\n"),
        "style": story.stylesheets.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join("\n"),
        "passages": passages,
    });
    Ok(serde_json::to_string_pretty(&value).unwrap_or_default() + "\n")
}

/// Compiles a story with a format.
pub fn compile(story: &Story, format: &Format, options: &Options) -> Result<Page, String> {
    let mut notes = vec![];
    let name = story.title.as_deref().map(str::trim).filter(|t| !t.is_empty()).ok_or("The story has no title: add a StoryTitle passage")?;
    let Some(ifid) = story.ifid.as_deref() else {
        let fresh = crate::ifid::new();
        return Err(format!(
            "The story has no IFID. Here's a new one; add it to the StoryData passage:\n\n:: StoryData\n{{\n  \"ifid\": \"{fresh}\",\n  \"format\": \"{}\",\n  \"format-version\": \"{}\"\n}}",
            format.name, format.version
        ));
    };

    // Passages (the story has already kept the last of a name, in the first one's place); scripts and stylesheets set
    // apart; Twine.private left out.
    let mut scripts: Vec<(String, String)> = story.scripts.iter().map(|c| (c.name.clone(), c.text.clone())).collect();
    let mut styles: Vec<(String, String)> = story.stylesheets.iter().map(|c| (c.name.clone(), c.text.clone())).collect();
    let mut passages: Vec<&Passage> = vec![];
    for passage in &story.passages {
        if matches!(passage.name.as_str(), "StoryTitle" | "StoryData") || passage.has_tag("Twine.private") {
            continue;
        }
        if passage.has_tag("script") {
            scripts.push((passage.name.clone(), passage.text.clone()));
            continue;
        }
        if passage.has_tag("stylesheet") {
            styles.push((passage.name.clone(), passage.text.clone()));
            continue;
        }
        passages.push(passage);
    }
    for duplicate in &story.duplicates {
        notes.push(format!("\"{}\" is declared more than once; the last one is used", duplicate.name));
    }

    let start = options.start.as_deref().or(story.start.as_deref()).unwrap_or("Start");
    let start_pid = passages
        .iter()
        .position(|p| p.name == start)
        .map(|i| i + 1)
        .ok_or_else(|| format!("The story starts at \"{start}\", and there's no passage by that name"))?;

    let zoom = story.zoom.unwrap_or(1.0);
    let zoom = if zoom.fract() == 0.0 { format!("{}", zoom as i64) } else { format!("{zoom}") };
    let mut data = String::new();
    data.push_str(&format!("<!-- UUID://{ifid}// -->"));
    data.push_str(&format!(
        "<tw-storydata name=\"{}\" startnode=\"{start_pid}\" creator=\"twee-tools\" creator-version=\"{}\" format=\"{}\" format-version=\"{}\" ifid=\"{}\" options=\"{}\" tags=\"\" zoom=\"{zoom}\" hidden>",
        escape(name),
        env!("CARGO_PKG_VERSION"),
        escape(&format.name),
        escape(&format.version),
        escape(ifid),
        if options.test { "debug" } else { "" },
    ));
    data.push_str(&format!(
        "<style role=\"stylesheet\" id=\"twine-user-stylesheet\" type=\"text/twine-css\">{}</style>",
        raw_code(&joined(&styles, "stylesheet"), "style")
    ));
    data.push_str(&format!(
        "<script role=\"script\" id=\"twine-user-script\" type=\"text/twine-javascript\">{}</script>",
        raw_code(&joined(&scripts, "script"), "script")
    ));
    for (tag, color) in &story.tag_colors {
        data.push_str(&format!("<tw-tag name=\"{}\" color=\"{}\"></tw-tag>", escape(tag), escape(color)));
    }
    for (index, passage) in passages.iter().enumerate() {
        let pid = index + 1;
        let position = passage.position.clone().unwrap_or_else(|| default_position(pid));
        let size = passage.size.clone().unwrap_or_else(|| "100,100".into());
        data.push_str(&format!(
            "<tw-passagedata pid=\"{pid}\" name=\"{}\" tags=\"{}\" position=\"{}\" size=\"{}\">{}</tw-passagedata>",
            escape(&passage.name),
            escape(&passage.tags.join(" ")),
            escape(&position),
            escape(&size),
            escape(passage.text.trim())
        ));
    }
    data.push_str("</tw-storydata>");

    if !format.source.contains("{{STORY_DATA}}") {
        notes.push(format!("{} {} has no {{{{STORY_DATA}}}} in its page, so the story isn't in it", format.name, format.version));
    }
    let mut html = format.source.replace("{{STORY_NAME}}", &escape(name)).replace("{{STORY_DATA}}", &data);
    if !options.head.is_empty() {
        let head = options.head.join("\n");
        match html.find("</head>") {
            Some(at) => html.insert_str(at, &format!("{head}\n")),
            None => notes.push("The format's page has no </head>, so the head file and modules weren't added".into()),
        }
    }
    Ok(Page { html, passages: passages.len(), notes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_ten_to_a_row() {
        assert_eq!(default_position(1), "100,100");
        assert_eq!(default_position(10), "1225,100");
        assert_eq!(default_position(11), "100,225");
    }

    #[test]
    fn escapes_html_and_closing_tags() {
        assert_eq!(escape(r#"<a href="x">Tom & 'Jo'</a>"#), "&lt;a href=&quot;x&quot;&gt;Tom &amp; &#39;Jo&#39;&lt;/a&gt;");
        assert_eq!(raw_code("var s = '</SCRIPT>'; </script", "script"), "var s = '<\\/SCRIPT>'; <\\/script");
    }

    #[test]
    fn joins_several_scripts_under_headers() {
        assert_eq!(joined(&[("a.js".into(), "one".into())], "script"), "one");
        assert_eq!(
            joined(&[("a.js".into(), "one".into()), ("b".into(), "two\n".into())], "script"),
            "/* twine-user-script #1: \"a.js\" */\none\n/* twine-user-script #2: \"b\" */\ntwo\n"
        );
    }
}
