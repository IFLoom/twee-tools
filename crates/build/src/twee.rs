//! A story as Twee 3: what `decompile` writes, from a published story, so it can live in files and git.

use story::{Passage, Story};

/// A passage name or tag with Twee's metacharacters escaped: `\`, `[`, `]`, `{`, `}`.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '\\' | '[' | ']' | '{' | '}') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn header(name: &str, tags: &[String], position: Option<&str>, size: Option<&str>) -> String {
    let mut out = format!(":: {}", escape(name));
    if !tags.is_empty() {
        out.push_str(&format!(" [{}]", tags.iter().map(|t| escape(t)).collect::<Vec<_>>().join(" ")));
    }
    let mut meta = serde_json::Map::new();
    if let Some(p) = position {
        meta.insert("position".into(), p.into());
    }
    if let Some(s) = size {
        meta.insert("size".into(), s.into());
    }
    if !meta.is_empty() {
        out.push_str(&format!(" {}", serde_json::Value::Object(meta)));
    }
    out
}

/// StoryData as Twee keeps it.
pub fn story_data(story: &Story) -> String {
    let mut data = serde_json::Map::new();
    if let Some(v) = &story.ifid {
        data.insert("ifid".into(), v.clone().into());
    }
    if let Some(v) = &story.format {
        data.insert("format".into(), v.clone().into());
    }
    if let Some(v) = &story.format_version {
        data.insert("format-version".into(), v.clone().into());
    }
    if let Some(v) = &story.start {
        data.insert("start".into(), v.clone().into());
    }
    if !story.tag_colors.is_empty() {
        let colors: serde_json::Map<String, serde_json::Value> = story.tag_colors.iter().map(|(k, v)| (k.clone(), v.clone().into())).collect();
        data.insert("tag-colors".into(), colors.into());
    }
    if let Some(zoom) = story.zoom {
        data.insert("zoom".into(), serde_json::json!(zoom));
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(data)).unwrap_or_default()
}

fn passage(p: &Passage) -> String {
    let mut out = header(&p.name, &p.tags, p.position.as_deref(), p.size.as_deref());
    out.push('\n');
    if !p.text.trim().is_empty() {
        out.push_str(p.text.trim_end());
        out.push('\n');
    }
    out
}

/// Code (CSS or JavaScript) as Twee passage text: a line that starts with `::` (`::selection`) would read back as a new
/// passage, so it's indented a space, which CSS and JavaScript don't mind.
fn code_text(text: &str) -> String {
    text.trim_end().lines().map(|line| if line.starts_with("::") { format!(" {line}") } else { line.to_string() }).collect::<Vec<_>>().join("\n")
}

/// What won't read back the same from the Twee this writes: passage text with a line starting `::`, and passages named
/// StoryTitle or StoryData (in a Twine 2 story they're ordinary passages; Twee keeps those names for the story's data).
pub fn notes(story: &Story) -> Vec<String> {
    let mut out = vec![];
    for duplicate in &story.duplicates {
        out.push(format!("\"{}\" was in the story more than once; only the last, the one that plays, was kept", duplicate.name));
    }
    for p in &story.passages {
        if matches!(p.name.as_str(), "StoryTitle" | "StoryData") {
            out.push(format!("The passage \"{}\" was left out: Twee uses that name for the story's own data", p.name));
        } else if p.text.lines().any(|l| l.starts_with("::")) {
            out.push(format!("\"{}\" has a line starting with ::, which Twee reads as a new passage", p.name));
        }
        if p.name.trim() != p.name || p.name.is_empty() {
            out.push(format!("\"{}\" starts or ends with a space, which Twee drops", p.name));
        }
    }
    out
}

fn kept(p: &&Passage) -> bool {
    !matches!(p.name.as_str(), "StoryTitle" | "StoryData")
}

/// The whole story as one Twee file: StoryTitle, StoryData, the stylesheet and JavaScript, then the passages.
pub fn story(story: &Story) -> String {
    let mut parts = vec![];
    if let Some(title) = &story.title {
        parts.push(format!(":: StoryTitle\n{title}\n"));
    }
    parts.push(format!(":: StoryData\n{}\n", story_data(story)));
    for code in &story.stylesheets {
        if !code.text.trim().is_empty() {
            parts.push(format!("{}\n{}\n", header(&code.name, &["stylesheet".into()], None, None), code_text(&code.text)));
        }
    }
    for code in &story.scripts {
        if !code.text.trim().is_empty() {
            parts.push(format!("{}\n{}\n", header(&code.name, &["script".into()], None, None), code_text(&code.text)));
        }
    }
    parts.extend(story.passages.iter().filter(kept).map(passage));
    parts.join("\n\n")
}

/// The story split in three: Twee (StoryTitle, StoryData and the passages), its JavaScript and its stylesheet.
pub fn split(story_in: &Story) -> (String, String, String) {
    let mut twee = vec![];
    if let Some(title) = &story_in.title {
        twee.push(format!(":: StoryTitle\n{title}\n"));
    }
    twee.push(format!(":: StoryData\n{}\n", story_data(story_in)));
    twee.extend(story_in.passages.iter().filter(kept).map(passage));
    let join = |codes: &[story::Code]| codes.iter().map(|c| c.text.trim_end().to_string()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    (twee.join("\n\n"), join(&story_in.scripts), join(&story_in.stylesheets))
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_twee_metacharacters() {
        assert_eq!(super::escape(r"A [b] {c} \d"), r"A \[b\] \{c\} \\d");
    }
}
