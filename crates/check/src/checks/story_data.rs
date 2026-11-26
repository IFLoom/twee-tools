//! The story as a whole: what reading it noticed, its IFID, start, format and version, title and passage names; and
//! SugarCube features newer than the version it declares.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::*;

/// The story as a whole: what reading it noticed, its IFID, start, format and version, title, and passage names.
pub(crate) fn story_level(ix: &Index, config: &Config, out: &mut Out) {
    let story = ix.story;
    for p in &story.problems {
        // Chapbook, Harlowe and Snowman match a link's passage exactly as written, spaces and all, so a name with spaces at
        // its ends is reached. (SugarCube trims it.)
        let exact = matches!(ix.format, crate::format::Format::Chapbook | crate::format::Format::Harlowe | crate::format::Format::Snowman);
        if exact && p.kind == "passage-name" && p.message.contains("starts or ends with a space") {
            continue;
        }
        let severity = if p.error { Severity::Error } else { Severity::Warning };
        out.in_file(severity, p.kind, p.file.clone(), p.line, p.passage.clone(), p.message.clone());
    }
    let data_broken = story.problems.iter().any(|p| p.kind == "story-data" && p.message.contains("isn't JSON"));
    let push = |out: &mut Out, severity: Severity, code: &'static str, message: String, subject: String, hint: Option<String>| {
        out.story_data(story, severity, code, message).about(subject).hint(hint);
    };

    // The IFID: required, an uppercase v4 UUID, and the story's own.
    match &story.ifid {
        None if !data_broken => push(
            out,
            Severity::Error,
            "ifid",
            "The story has no IFID, which identifies it".into(),
            "IFID".into(),
            Some("Twine gives every story one; any version 4 UUID in capitals will do".into()),
        ),
        None => {}
        Some(ifid) => {
            if !is_v4_uuid(ifid) {
                let hint = if is_v4_uuid(&ifid.to_uppercase()) {
                    Some(format!("in capitals: {}", ifid.to_uppercase()))
                } else {
                    Some("an IFID is a version 4 UUID in capitals: 8-4-4-4-12 hex digits, the third group starting with 4".into())
                };
                push(
                    out,
                    Severity::Error,
                    "ifid",
                    format!("The IFID \"{ifid}\" isn't a v4 UUID in capitals, as the Twee and Twine specs require"),
                    ifid.clone(),
                    hint,
                );
            } else if ["D674C58C-DEFA-4F70-B7A2-27742230C0FC", "6D509890-1CA5-49DF-BFB6-5CA35B8DE2AC"].contains(&ifid.as_str()) {
                push(
                    out,
                    Severity::Warning,
                    "ifid",
                    format!("The IFID {ifid} is the one in the Twee spec's examples, so other stories share it"),
                    ifid.clone(),
                    Some("make a new one: any v4 UUID in capitals".into()),
                );
            }
        }
    }

    // A start passage, which isn't a code passage.
    let start_problem = story.problems.iter().any(|p| p.kind == "start");
    match &story.start {
        None if !start_problem && !data_broken => {
            let message = match story.form {
                story::Form::Twee => "The story has no start passage: StoryData has no \"start\", and there's no passage named Start",
                story::Form::Html => "The story has no start passage: <tw-storydata> has no startnode",
            };
            push(out, Severity::Error, "start", message.into(), "start".into(), None);
        }
        Some(start) => {
            if let Some(passage) = story.passage(start) {
                let code_tag = passage
                    .tags
                    .iter()
                    .find(|t| matches!(t.as_str(), "widget" | "init" | "script" | "stylesheet" | "Twine.image" | "Twine.audio" | "Twine.video" | "Twine.vtt"));
                let code_passage = sugarcube::SPECIAL_PASSAGES.contains(&start.as_str()) && !["StoryTitle"].contains(&start.as_str());
                if ix.format.sugarcube() && (code_tag.is_some() || code_passage) {
                    let why = code_tag.map(|t| format!("it's tagged {t}")).unwrap_or_else(|| "it's one of SugarCube's special passages".into());
                    push(out, Severity::Error, "start", format!("The story starts at \"{start}\", and {why}, so it can't be shown"), start.clone(), None);
                }
            }
        }
        None => {}
    }

    // The story format and its version.
    const FORMATS: &[&str] = &["Harlowe", "SugarCube", "Chapbook", "Snowman"];
    const PROOFING: &[&str] = &["Paperthin", "Illume"];
    const TWINE1: &[&str] = &["Jonah", "Sugarcane", "Responsive"];
    match story.format.as_deref() {
        None if !data_broken => push(
            out,
            Severity::Warning,
            "story-format",
            "The story names no story format, so Twine will import it as Harlowe".into(),
            "format".into(),
            Some("add \"format\" to StoryData, such as \"SugarCube\"".into()),
        ),
        None => {}
        Some(format) => {
            let exact = |list: &[&str]| list.contains(&format);
            let near = |list: &[&'static str]| list.iter().find(|f| f.eq_ignore_ascii_case(format)).copied();
            if exact(PROOFING) || near(PROOFING).is_some() {
                push(
                    out,
                    Severity::Error,
                    "story-format",
                    format!("The story format is {format}, a proofing format for printing a story, not playing it"),
                    format.into(),
                    None,
                );
            } else if exact(TWINE1) || near(TWINE1).is_some() {
                push(
                    out,
                    Severity::Warning,
                    "story-format",
                    format!("{format} is a Twine 1 story format, which shouldn't be used any more"),
                    format.into(),
                    Some("SugarCube is the closest Twine 2 format".into()),
                );
            } else if let Some(right) = near(FORMATS).filter(|r| *r != format) {
                push(
                    out,
                    Severity::Warning,
                    "story-format",
                    format!("The story format \"{format}\" is written with other capitals than Twine's \"{right}\""),
                    format.into(),
                    Some(format!("write \"{right}\"")),
                );
            } else if !exact(FORMATS) {
                push(
                    out,
                    Severity::Note,
                    "story-format",
                    format!("\"{format}\" isn't one of the formats Twine comes with"),
                    format.into(),
                    Some("if it's a format you added to Twine, this is fine".into()),
                );
            }
        }
    }
    if let Some(version) = &story.format_version
        && parse_version(version).is_none()
    {
        {
            push(
                out,
                Severity::Warning,
                "story-format",
                format!("The format version \"{version}\" isn't x.y.z, as story formats are versioned"),
                version.clone(),
                None,
            );
        }
    }
    version_gate(ix, config, out);

    // The title.
    match story.title.as_deref().map(str::trim) {
        None | Some("") => push(out, Severity::Error, "story-title", "The story has no title".into(), "title".into(), None),
        Some(title) if title.eq_ignore_ascii_case("Untitled Story") => {
            push(out, Severity::Warning, "story-title", "The story is still called \"Untitled Story\"".into(), title.into(), None)
        }
        _ => {}
    }
    if let Some((text, file, line)) = &story.title_passage
        && ["<<", "[[", "''", "//", "@@", "<"].iter().any(|m| text.contains(m))
        && ix.format.sugarcube()
    {
        {
            let message = "StoryTitle has markup or code in it; SugarCube makes the save key from it, so keep it plain text".to_string();
            out.in_file(Severity::Warning, "story-title", file.clone(), *line, Some("StoryTitle".into()), message)
                .about("StoryTitle")
                .hint(Some("put styled text in StoryDisplayTitle".into()));
        }
    }

    // Passage names link markup can't reach.
    for (_, passage, _parse) in ix.passages() {
        let bad: Vec<&str> = ["[", "]", "|", "->", "<-"].into_iter().filter(|m| passage.name.contains(m)).collect();
        if !bad.is_empty() {
            let message = format!("\"{}\" has {} in its name, which link markup reads as its own, so [[…]] can't reach it", passage.name, bad.join(" and "));
            out.in_file(Severity::Warning, "passage-name", passage.file.clone(), passage.line, Some(passage.name.clone()), message).about(passage.name.clone());
        }
    }
}

/// SugarCube features newer than the version the story declares, each reported once.
pub(crate) fn version_gate(ix: &Index, _config: &Config, out: &mut Out) {
    let story = ix.story;
    if !ix.format.sugarcube() {
        return;
    }
    let Some(declared) = story.format_version.as_deref().and_then(parse_version) else {
        return;
    };
    if declared.0 != 2 {
        return;
    }
    let mut reported: HashSet<&str> = HashSet::new();
    let mut report = |out: &mut Out, feature: &'static str, since: (u32, u32, u32), file: PathBuf, line: usize, passage: Option<String>| {
        if since > declared && reported.insert(feature) {
            let message = format!(
                "{feature} came in SugarCube {}.{}.{}, and the story says it's for {}.{}.{}",
                since.0, since.1, since.2, declared.0, declared.1, declared.2
            );
            let hint = Some(format!("raise format-version to {}.{}.{} or later, or don't use it", since.0, since.1, since.2));
            out.in_file(Severity::Error, "format-version", file, line, passage, message).about(feature).hint(hint);
        }
    };
    for (_, passage, parse) in ix.passages() {
        let markup = &parse.markup;
        for tag in parse.tags.iter().filter(|t| !t.closing) {
            if let Some(&(feature, since)) = sugarcube::SINCE_MACROS.iter().find(|(name, _)| *name == tag.name).map(|(_, f)| f) {
                report(out, feature, since, passage.file.clone(), passage.line_at(tag.start), Some(passage.name.clone()));
            }
        }
        for tag in &passage.tags {
            if let Some(&(feature, since)) = sugarcube::SINCE_TAGS.iter().find(|(name, _)| name == tag).map(|(_, f)| f) {
                report(out, feature, since, passage.file.clone(), passage.line, Some(passage.name.clone()));
            }
        }
        if let Some(&(feature, since)) = sugarcube::SINCE_PASSAGES.iter().find(|(name, _)| *name == passage.name).map(|(_, f)| f) {
            report(out, feature, since, passage.file.clone(), passage.line, Some(passage.name.clone()));
        }
        let code = &parse.code;
        if passage.has_tag("widget")
            && let Some(at) = code.find("_args")
        {
            report(out, "_args", (2, 36, 0), passage.file.clone(), passage.line_at(at), Some(passage.name.clone()));
        }
        if let Some(at) = markup.find(" @").filter(|&at| {
            markup[at + 2..].starts_with(|c: char| c.is_ascii_alphabetic()) && markup[at..].split_whitespace().next().is_some_and(|w| w.contains('='))
        }) {
            report(out, "@attribute directives", (2, 21, 0), passage.file.clone(), passage.line_at(at), Some(passage.name.clone()));
        }
    }
    for code in &story.scripts {
        for &(needle, feature, since) in sugarcube::SINCE_CODE {
            if let Some(at) = code.text.find(needle) {
                let line = code.line + code.text[..at].matches('\n').count();
                report(out, feature, since, code.file.clone(), line, None);
            }
        }
    }
}
