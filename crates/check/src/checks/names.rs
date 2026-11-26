//! SugarCube's own rules: special names, code passages, widget and macro names, removed and deprecated APIs, Config.

use std::path::PathBuf;

use story::Passage;

use crate::config::Config;
use crate::index::Index;
use crate::macros_defined::MacroKind;
use crate::out::Out;
use crate::scan::{self};
use crate::*;

/// SugarCube's own rules: special names, code passages, widget and macro names, removed and deprecated APIs, Config.
pub(crate) fn sugarcube_rules(ix: &Index, _config: &Config, out: &mut Out) {
    let story = ix.story;
    let declared = story.format_version.as_deref().and_then(parse_version);
    // 2.37 removed what 2.36 deprecated; a story for an older SugarCube still has them, as deprecated.
    let removed_applies = declared.is_none_or(|v| v >= (2, 37, 0));
    let at = |out: &mut Out, p: &Passage, offset: usize, severity: Severity, code: &'static str, message: String, subject: String, hint: Option<String>| {
        out.at(severity, code, p, offset, message).about(subject).hint(hint);
    };

    // Special names written almost right do nothing: Storyinit, Story Init, tag Widget, NOBR, twine.image.
    let squash = |s: &str| s.to_lowercase().replace([' ', '_', '-'], "");
    for (_, passage, parse) in ix.passages() {
        if !sugarcube::SPECIAL_PASSAGES.contains(&passage.name.as_str())
            && let Some(right) = sugarcube::SPECIAL_PASSAGES
                .iter()
                .find(|s| squash(s) == squash(&passage.name) || (passage.name.len() >= 8 && props::edits(s, &passage.name) == 1))
        {
            let message = format!("\"{}\" looks like SugarCube's special passage {right}, but special names are exact, so it does nothing", passage.name);
            at(out, passage, 0, Severity::Error, "special-name", message, passage.name.clone(), Some(format!("name it {right}")));
        }
        for tag in &passage.tags {
            let known = sugarcube::SPECIAL_TAGS.iter().chain(crate::format::TWINE_TAGS).chain(["nobr", "bookmark"].iter());
            if !known.clone().any(|k| k == tag)
                && let Some(right) = known.into_iter().find(|k| squash(k) == squash(tag))
            {
                let message =
                    format!("The tag \"{tag}\" on \"{}\" looks like SugarCube's {right}, but special tags are exact, so it does nothing", passage.name);
                at(out, passage, 0, Severity::Error, "special-name", message, tag.clone(), Some(format!("tag it {right}")));
            }
        }
        // A special passage with a code tag, and a widget outside a widget passage.
        let code_tag = passage.tags.iter().find(|t| sugarcube::CODE_TAGS.contains(&t.as_str()));
        if let Some(tag) = code_tag.filter(|_| sugarcube::SPECIAL_PASSAGES.contains(&passage.name.as_str())) {
            let message = format!("{} is a special passage, and it's tagged {tag}; SugarCube says never to combine the two", passage.name);
            at(out, passage, 0, Severity::Error, "code-passage", message, passage.name.clone(), Some(format!("move the {tag} code into its own passage")));
        }
        let markup = &parse.markup;
        let tags = &parse.tags;
        if !passage.has_tag("widget") {
            for tag in tags.iter().filter(|t| t.name == "widget" && !t.closing) {
                let message = "<<widget>> outside a passage tagged widget, so the widget is lost when the page reloads".to_string();
                at(out, passage, tag.start, Severity::Error, "code-passage", message, "<<widget>>".into(), Some("tag the passage widget".into()));
            }
        }
        // Twine 2 stories have no script or stylesheet tags: the code in such a passage never runs.
        if story.form == story::Form::Html
            && let Some(tag) = passage.tags.iter().find(|t| matches!(t.as_str(), "script" | "stylesheet"))
        {
            let message = format!(
                "\"{}\" is tagged {tag}, which only Twine 1 and Twee use; in a Twine 2 story it's an ordinary passage, so its code never runs",
                passage.name
            );
            let home = if tag == "script" { "Story JavaScript" } else { "Story Stylesheet" };
            at(out, passage, 0, Severity::Error, "twine1-tags", message, tag.clone(), Some(format!("move it to the {home}")));
        }
        // Passages that show nothing: words there never reach the player.
        if sugarcube::NO_OUTPUT.contains(&passage.name.as_str()) || passage.has_tag("init") {
            let mut prose = markup.clone();
            for tag in tags.iter().rev() {
                prose.replace_range(tag.start..tag.end, &" ".repeat(tag.end - tag.start));
            }
            let prose = strip_html(&prose);
            if let Some(offset) = prose.find(|c: char| c.is_alphanumeric()) {
                let words: String = prose[offset..].chars().take(40).take_while(|c| *c != '\n').collect();
                let message = format!("{} shows nothing, so \"{}…\" never reaches the player", passage.name, words.trim());
                // `// words` looks like a comment, and in SugarCube's markup it isn't one.
                let slashes = prose[..offset].trim_end().ends_with("//");
                let hint = slashes.then(|| "`//` isn't a comment in SugarCube's markup; write /* … */".to_string());
                at(out, passage, offset, Severity::Warning, "no-output", message, passage.name.clone(), hint);
            }
        }
        // Removed and deprecated macros, and the 2.0 back::linktext form.
        for tag in tags.iter().filter(|t| !t.closing) {
            if let Some(&(_, instead)) = sugarcube::REMOVED_MACROS.iter().find(|(m, _)| *m == tag.name) {
                let (severity, code, word) = if removed_applies {
                    (Severity::Error, "removed-api", "was removed in SugarCube 2.37")
                } else {
                    (Severity::Note, "deprecated", "is deprecated")
                };
                at(out, passage, tag.start, severity, code, format!("<<{}>> {word}", tag.name), format!("<<{}>>", tag.name), Some(instead.into()));
            }
            if let Some(&(_, instead)) = sugarcube::DEPRECATED_MACROS.iter().find(|(m, _)| *m == tag.name) {
                at(
                    out,
                    passage,
                    tag.start,
                    Severity::Note,
                    "deprecated",
                    format!("<<{}>> is deprecated", tag.name),
                    format!("<<{}>>", tag.name),
                    Some(instead.into()),
                );
            }
            if matches!(tag.name.as_str(), "back" | "return") && tag.args.starts_with("::") {
                at(
                    out,
                    passage,
                    tag.start,
                    Severity::Error,
                    "removed-api",
                    format!("<<{}::…>> was removed in SugarCube 2.0", tag.name),
                    format!("<<{}::>>", tag.name),
                    Some(format!("<<{} \"link text\">>", tag.name)),
                );
            }
            // `is not` no longer means `isnot` (2.37): `$a is not "x"` compares $a with !"x".
            if removed_applies && matches!(tag.name.as_str(), "if" | "elseif" | "set" | "run" | "print" | "=" | "-") {
                let code = &tag.args;
                if let Some(offset) = find_word_pair(code, "is", "not") {
                    let message = format!("<<{}>> has `is not`, which SugarCube 2.37 reads as `=== !…`, not as `isnot`", tag.name);
                    at(out, passage, tag.args_start + offset, Severity::Error, "removed-api", message, "is not".into(), Some("write isnot".into()));
                }
            }
        }
    }

    // Widget and macro names.
    for defined in ix.macros() {
        let name = &defined.name;
        let builtin = sugarcube::CONTAINERS.contains(&name.as_str()) || sugarcube::SINGLES.contains(&name.as_str());
        let widget = defined.kind == MacroKind::Widget;
        let valid = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        let what = if widget { "widget" } else { "macro" };
        let problem = if builtin {
            Some(format!("The {what} <<{name}>> has the name of one of SugarCube's own macros, which is an error when the story starts"))
        } else if !valid {
            Some(format!("The {what} name \"{name}\" isn't allowed: a letter, then letters, digits, _ and -"))
        } else {
            None
        };
        if let Some(message) = problem {
            out.in_file(Severity::Error, "macro-name", defined.file.clone(), defined.line, defined.passage.clone(), message).about(format!("<<{name}>>"));
        }
    }

    // Removed and deprecated APIs, and Config, in the story's JavaScript and in passages' code.
    // Only code: the story's JavaScript, and in passages macro arguments and script bodies, never prose ("his tale.").
    let mut texts: Vec<(Option<&Passage>, PathBuf, usize, String)> = story.scripts.iter().map(|c| (None, c.file.clone(), c.line, c.text.clone())).collect();
    texts.extend(ix.passages().map(|(_, p, parse)| (Some(p), p.file.clone(), p.body_line, code_only(&p.text, &parse.tags))));
    let config_names: Vec<String> = sugarcube::CONFIG.iter().map(|c| c.to_string()).collect();
    let names = &ix.names.set;
    for (passage, file, first_line, text) in &texts {
        let place = |offset: usize| first_line + text[..offset.min(text.len())].matches('\n').count();
        let push = |out: &mut Out, offset: usize, severity: Severity, code: &'static str, message: String, subject: String, hint: Option<String>| {
            out.in_file(severity, code, file.clone(), place(offset), passage.map(|p| p.name.clone()), message).about(subject).hint(hint);
        };
        // Names inside strings aren't uses: "a tale." in a message.
        let bare = blank_strings(text);
        for &(needle, instead) in sugarcube::REMOVED_API {
            if let Some(offset) = find_api(&bare, needle) {
                let (severity, code, word) = if removed_applies {
                    (Severity::Error, "removed-api", "was removed in SugarCube 2.37")
                } else {
                    (Severity::Note, "deprecated", "is deprecated")
                };
                push(
                    out,
                    offset,
                    severity,
                    code,
                    format!("{} {word}", needle.trim_end_matches('(')),
                    needle.trim_end_matches('(').into(),
                    Some(instead.into()),
                );
            }
        }
        for &(needle, instead) in sugarcube::DEPRECATED_API {
            if let Some(offset) = find_api(&bare, needle) {
                push(
                    out,
                    offset,
                    Severity::Note,
                    "deprecated",
                    format!("{} is deprecated", needle.trim_end_matches('(')),
                    needle.trim_end_matches('(').into(),
                    Some(instead.into()),
                );
            }
        }
        // Config.x.y: one SugarCube has; Setting only in the story's JavaScript; some values that break.
        for (offset, _) in bare.match_indices("Config.") {
            if text[..offset].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '.') {
                continue;
            }
            let path: String = text[offset + 7..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.').collect();
            let path = path.trim_end_matches('.');
            let known = sugarcube::CONFIG.iter().any(|c| *c == path || c.starts_with(&format!("{path}.")));
            if !path.is_empty() && !known {
                let hint = props::closest(path, &config_names).map(|c| format!("did you mean Config.{c}?"));
                push(
                    out,
                    offset,
                    Severity::Error,
                    "config",
                    format!("Config.{path} isn't one of SugarCube's settings, so setting it does nothing"),
                    format!("Config.{path}"),
                    hint,
                );
            }
            let value = text[offset + 7 + path.len()..].trim_start();
            let assigned = value.strip_prefix('=').filter(|v| !v.starts_with('=')).map(str::trim_start);
            match (path, assigned) {
                ("passages.transitionOut", Some(v)) if v.starts_with("true") || v.starts_with("false") => push(
                    out,
                    offset,
                    Severity::Error,
                    "config",
                    "Config.passages.transitionOut doesn't take true or false since SugarCube 2.0".into(),
                    "Config.passages.transitionOut".into(),
                    Some("give the outgoing transition's duration in milliseconds, or a CSS property name".into()),
                ),
                ("debug", Some(v)) if v.starts_with("true") => push(
                    out,
                    offset,
                    Severity::Warning,
                    "config",
                    "Config.debug is true, so players get the debug views".into(),
                    "Config.debug".into(),
                    Some("turn it off before publishing".into()),
                ),
                ("passages.start", Some(v)) => {
                    if let Some((_, target, _)) = scan::strings(v).into_iter().next().filter(|(o, _, _)| *o == 1)
                        && !names.contains(target.as_str())
                    {
                        push(
                            out,
                            offset,
                            Severity::Error,
                            "config",
                            format!("Config.passages.start is \"{target}\", and there's no passage by that name"),
                            "Config.passages.start".into(),
                            suggest(&target, names),
                        );
                    }
                }
                _ => {}
            }
        }
        if passage.is_some() {
            if let Some(offset) = find_api(&bare, "Setting.") {
                push(
                    out,
                    offset,
                    Severity::Error,
                    "config",
                    "Setting is used in a passage; SugarCube only sets settings up from the story's JavaScript".into(),
                    "Setting".into(),
                    Some("move it to the Story JavaScript".into()),
                );
            }
            let startup = passage.is_some_and(|p| p.name == "StoryInit" || p.has_tag("init"));
            if !startup && let Some(offset) = find_api(&bare, "State.prng.init(") {
                push(
                    out,
                    offset,
                    Severity::Error,
                    "config",
                    "State.prng.init() is called after the story has started, which throws".into(),
                    "State.prng.init".into(),
                    Some("call it in the Story JavaScript or StoryInit".into()),
                );
            }
        }
    }
}
