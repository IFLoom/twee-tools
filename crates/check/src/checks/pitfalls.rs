//! SugarCube's markup and macro pitfalls, as its docs give them: `=` in `<<if>>`, receivers not quoted, expressions passed
//! as separate arguments, `<<default>>` not last, names that aren't story variables, expressions in naked variables,
//! `def` without parentheses, code after `<<goto>>`, loops without `<<capture>>`, audio IDs, link targets, setters and
//! attribute code, and Harlowe in a SugarCube story (or the other way round).

use std::collections::HashSet;

use story::Passage;

use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::scan::{self, Arg, MacroTag};
use crate::{Severity, js};

/// Macros whose receiver, the variable they set, comes first and must be quoted: `<<textbox "$name" "">>`.
const RECEIVERS: &[&str] = &["textbox", "textarea", "numberbox", "checkbox", "radiobutton", "cycle", "listbox"];

/// Macros that make something the player clicks, whose body runs later.
const INTERACTIVE: &[&str] = &["link", "button", "linkreplace", "linkappend", "linkprepend", "click"];

/// Macros that take separate arguments, not one expression: an unquoted `+` or a comma between them is a mistake.
const DISCRETE: &[&str] = &[
    "link",
    "button",
    "goto",
    "include",
    "textbox",
    "textarea",
    "numberbox",
    "checkbox",
    "radiobutton",
    "listbox",
    "audio",
    "cacheaudio",
    "linkreplace",
    "linkappend",
    "linkprepend",
    "back",
    "return",
    "type",
    "timed",
    "repeat",
    "addclass",
    "removeclass",
    "toggleclass",
    "append",
    "prepend",
    "replace",
    "copy",
    "remove",
    "playlist",
    "createaudiogroup",
    "createplaylist",
    "track",
    "option",
];

/// Audio groups SugarCube defines itself.
const AUDIO_GROUPS: &[&str] = &[":all", ":looped", ":muted", ":paused", ":playing", ":stopped", ":not"];

/// Harlowe's macro calls, `(set:`, `(if:)`, which a SugarCube story shows as text.
const HARLOWE: &[&str] = &[
    "set",
    "put",
    "if",
    "else-if",
    "elseif",
    "unless",
    "else",
    "link-goto",
    "go-to",
    "goto",
    "link",
    "link-reveal",
    "click",
    "print",
    "display",
    "either",
    "replace",
    "append",
    "prepend",
    "for",
    "loop",
    "cycling-link",
    "dropdown",
    "history",
    "visited",
    "random",
    "nth",
    "a",
    "dm",
    "ds",
    "link-reveal-goto",
    "after",
    "live",
    "stop",
    "show",
    "hide",
    "text-colour",
    "text-color",
    "text-style",
];

pub(crate) fn pitfalls(ix: &Index, _config: &Config, out: &mut Out) {
    let story = ix.story;
    let at = |out: &mut Out, p: &Passage, offset: usize, severity: Severity, code: &'static str, message: String, subject: String, hint: Option<String>| {
        out.at(severity, code, p, offset, message).about(subject).hint(hint);
    };
    let names = &ix.names.set;
    let sugarcube = ix.format.sugarcube();

    // A Harlowe (or Chapbook or Snowman) story with SugarCube macros in it, and the other way round.
    if !sugarcube {
        for (_, passage, parse) in ix.passages() {
            let _markup = &parse.markup;
            if let Some(tag) = parse.tags.iter().find(|t| matches!(t.name.as_str(), "set" | "if" | "link" | "goto" | "print" | "include")) {
                let format = story.format.clone().unwrap_or_default();
                let message = format!("<<{}>> is a SugarCube macro, and the story's format is {format}, which shows it as text", tag.name);
                at(
                    out,
                    passage,
                    tag.start,
                    Severity::Error,
                    "wrong-format",
                    message,
                    format!("<<{}>>", tag.name),
                    Some("set the format to SugarCube, or write it in the story's own format".into()),
                );
                break;
            }
        }
        return;
    }

    // Which audio tracks, groups and playlists the story sets up, unless its JavaScript does that itself.
    let audio_from_code = story.scripts.iter().any(|c| c.text.contains("SimpleAudio.tracks.add") || c.text.contains("SimpleAudio.groups.add"));
    let mut tracks: HashSet<String> = HashSet::new();
    let mut groups: HashSet<String> = AUDIO_GROUPS.iter().map(|g| g.to_string()).collect();
    for (_, _passage, parse) in ix.passages() {
        for tag in parse.tags.iter().filter(|t| !t.closing) {
            let first = scan::args(&tag.args).into_iter().next();
            match (tag.name.as_str(), first) {
                ("cacheaudio" | "createplaylist", Some(Arg::Str(id))) => {
                    tracks.insert(id);
                }
                ("createaudiogroup", Some(Arg::Str(id))) => {
                    groups.insert(id);
                }
                _ => {}
            }
        }
    }

    for (_, passage, parse) in ix.passages() {
        let markup = &parse.markup;
        let tags = &parse.tags;
        let mut switches: Vec<(bool, usize)> = vec![];
        for (index, tag) in tags.iter().enumerate() {
            if tag.closing {
                if tag.name == "switch" {
                    switches.pop();
                }
                continue;
            }
            let args = scan::args(&tag.args);
            let name = tag.name.as_str();

            // = (or `to`) in a condition assigns, and SugarCube warns of it only in debug mode.
            if matches!(name, "if" | "elseif")
                && let Some(offset) = lone_assignment(&tag.args)
            {
                let message = format!("<<{name}>> has an `=`, which sets the variable rather than comparing it");
                at(
                    out,
                    passage,
                    tag.args_start + offset,
                    Severity::Error,
                    "if-assignment",
                    message,
                    format!("<<{name}>>"),
                    Some("compare with `is` (or ===)".into()),
                );
            }

            // A receiver must be quoted: `<<textbox "$name">>`, not `<<textbox $name>>`.
            if RECEIVERS.contains(&name)
                && let Some(Arg::Bare(word)) = args.first()
                && word.starts_with(['$', '_'])
            {
                let message = format!("<<{name}>>'s variable {word} isn't quoted, so the macro gets its value, not its name");
                at(out, passage, tag.start, Severity::Error, "receiver", message, format!("<<{name}>>"), Some(format!("write \"{word}\"")));
            }

            // Separate arguments: an expression must be one argument (in backquotes), and commas don't separate.
            if DISCRETE.contains(&name) {
                let raw = tag.args.trim();
                if args.iter().any(|a| matches!(a, Arg::Bare(w) if w == "+" || w.starts_with('+') || w.ends_with('+'))) {
                    let message = format!("<<{name}>> is given an expression in pieces, so it gets each piece as its own argument");
                    at(
                        out,
                        passage,
                        tag.start,
                        Severity::Error,
                        "macro-args",
                        message,
                        format!("<<{name}>>"),
                        Some("put the expression in backquotes: `\"Wake \" + $friend`".into()),
                    );
                } else if comma_between_args(raw) {
                    let message = format!("<<{name}>>'s arguments are separated by commas, as Harlowe does; SugarCube separates them with spaces");
                    at(out, passage, tag.start, Severity::Error, "macro-args", message, format!("<<{name}>>"), Some("drop the commas".into()));
                }
            }
            // A macro as an argument (markup inside a quoted text argument is fine: link text is markup).
            if !matches!(name, "script" | "run" | "set" | "print" | "=" | "-") && outside_strings(&tag.args).contains("<<") {
                let message = format!("<<{name}>> has a macro in its arguments; a macro can't be an argument");
                at(out, passage, tag.start, Severity::Error, "macro-args", message, format!("<<{name}>>"), None);
            }

            // <<back>> and <<return>> go back a passage: they don't hand back a value, as JavaScript's return does.
            if matches!(name, "back" | "return")
                && let Some(arg) = args.first()
                && !matches!(arg, Arg::Str(_) | Arg::Link(..) | Arg::Image(..))
            {
                let message = format!("<<{name}>> goes back to the previous passage; it doesn't return a value, as JavaScript's return does");
                at(
                    out,
                    passage,
                    tag.start,
                    Severity::Error,
                    "macro-args",
                    message,
                    format!("<<{name}>>"),
                    Some("to hand back a value, use a JavaScript function (setup.f = function () { … })".into()),
                );
            }

            // <<default>> is the last case, and there's one.
            match name {
                "switch" => switches.push((false, 0)),
                "default" => {
                    if let Some(last) = switches.last_mut() {
                        if last.0 {
                            at(out, passage, tag.start, Severity::Error, "switch", "<<switch>> has two <<default>>s".into(), "<<default>>".into(), None);
                        }
                        *last = (true, tag.start);
                    }
                }
                "case" => {
                    if let Some(&(true, default_at)) = switches.last() {
                        at(
                            out,
                            passage,
                            default_at,
                            Severity::Error,
                            "switch",
                            "<<default>> comes before a <<case>>; it must be the last".into(),
                            "<<default>>".into(),
                            None,
                        );
                        if let Some(last) = switches.last_mut() {
                            last.1 = usize::MAX;
                        }
                    }
                }
                _ => {}
            }

            // Names that aren't story variables: $1st, $my-var.
            if !matches!(name, "script") {
                for (offset, bad, why) in bad_variable_names(&tag.args) {
                    at(
                        out,
                        passage,
                        tag.args_start + offset,
                        Severity::Error,
                        "variable-name",
                        format!("{bad} isn't a story variable's name: {why}"),
                        bad.clone(),
                        None,
                    );
                }
            }

            // def and ndef bind loosely: wrap them in parentheses.
            if matches!(name, "if" | "elseif" | "set" | "print" | "=" | "-")
                && let Some(offset) = loose_def(&tag.args)
            {
                let message = format!("<<{name}>> mixes def/ndef with other operators without parentheses, and they bind more loosely than you'd think");
                at(
                    out,
                    passage,
                    tag.args_start + offset,
                    Severity::Note,
                    "def-precedence",
                    message,
                    format!("<<{name}>>"),
                    Some("write (def $x) and …".into()),
                );
            }

            // <<goto>> doesn't stop the passage: what follows it in the same block still runs.
            // (`<<endif>>` is the old way to write `<</if>>`, so it closes too.)
            let closes = |t: &MacroTag| t.closing || t.name.strip_prefix("end").is_some_and(|rest| crate::sugarcube::CONTAINERS.contains(&rest));
            if name == "goto"
                && let Some(next) = tags.get(index + 1)
                && !closes(next)
                && !matches!(next.name.as_str(), "else" | "elseif" | "case" | "default")
            {
                let message = format!("<<{}>> comes after <<goto>> and still runs, as <<goto>> doesn't stop the passage", next.name);
                at(
                    out,
                    passage,
                    next.start,
                    Severity::Warning,
                    "goto-continues",
                    message,
                    format!("<<{}>>", next.name),
                    Some("move it before the <<goto>>".into()),
                );
            }

            // A link inside <<for>> uses the loop's variable later, when it has its last value: <<capture>> it.
            if name == "for" {
                let vars = loop_variables(&tag.args);
                let end = matching_close(tags, index);
                let mut captured: Vec<(usize, Vec<String>)> = vec![];
                for (inner_index, inner) in tags.iter().enumerate().take(end).skip(index + 1) {
                    if inner.name == "capture" && !inner.closing {
                        captured.push((matching_close(tags, inner_index), loop_variables(&inner.args)));
                    }
                    captured.retain(|(close, _)| *close > inner_index);
                    if inner.closing || !INTERACTIVE.contains(&inner.name.as_str()) {
                        continue;
                    }
                    let close = matching_close(tags, inner_index);
                    let body = &passage.text[inner.end.min(passage.text.len())..tags.get(close).map(|t| t.start).unwrap_or(passage.text.len())];
                    for var in &vars {
                        let held = captured.iter().any(|(_, names)| names.contains(var));
                        if !held && uses_name(body, var) {
                            let message = format!("<<{}>> inside <<for>> uses {var} when it's clicked, and by then {var} has its last value", inner.name);
                            at(
                                out,
                                passage,
                                inner.start,
                                Severity::Error,
                                "capture",
                                message,
                                var.clone(),
                                Some(format!("wrap it in <<capture {var}>>…<</capture>>")),
                            );
                        }
                    }
                }
            }

            // Audio: tracks and playlists set up first; group IDs start with a colon, and aren't SugarCube's own.
            if !audio_from_code
                && matches!(name, "audio" | "playlist")
                && let Some(Arg::Str(ids)) = args.first()
            {
                for id in ids.split_whitespace() {
                    let known = if id.starts_with(':') { groups.contains(id) || id.starts_with(":not(") } else { tracks.contains(id) };
                    if !known {
                        let what = if id.starts_with(':') {
                            "group"
                        } else if name == "playlist" {
                            "playlist"
                        } else {
                            "track"
                        };
                        let message = format!("<<{name}>> plays the {what} \"{id}\", which nothing sets up");
                        let hint = props_closest(id, &tracks, &groups);
                        at(out, passage, tag.start, Severity::Error, "audio", message, id.to_string(), hint);
                    }
                }
            }
            if name == "createaudiogroup"
                && let Some(Arg::Str(id)) = args.first()
            {
                if !id.starts_with(':') {
                    at(
                        out,
                        passage,
                        tag.start,
                        Severity::Error,
                        "audio",
                        format!("The audio group \"{id}\" doesn't start with a colon"),
                        id.clone(),
                        Some(format!("name it \":{id}\"")),
                    );
                } else if AUDIO_GROUPS.contains(&id.as_str()) {
                    at(out, passage, tag.start, Severity::Error, "audio", format!("The audio group {id} is one of SugarCube's own"), id.clone(), None);
                }
            }

            // Code in macros SugarCube evaluates later: link setters and attribute directives are checked below.
        }

        // Expressions in naked variables aren't evaluated: $items[_i + 1], $name.toUpperCase().
        let mut prose = markup.clone();
        for tag in tags.iter().rev() {
            prose.replace_range(tag.start..tag.end, &" ".repeat(tag.end - tag.start));
        }
        for (offset, what) in naked_expressions(&prose) {
            let message = format!("{what} is in the text as a naked variable, which doesn't evaluate expressions or method calls");
            at(out, passage, offset, Severity::Warning, "naked-variable", message, what.clone(), Some(format!("write <<print {what}>>")));
        }

        // Link markup: an expression as the target in a Twine 2 story; a setter on an outside link; setter code.
        let (links, _) = scan::links(markup);
        for link in &links {
            if story.form == story::Form::Html && link.target.starts_with(['$', '_']) && names.contains(link.target.as_str()) {
                let message = format!("[[…|{}]] made Twine 2 create an empty passage named \"{}\"", link.target, link.target);
                at(
                    out,
                    passage,
                    link.start,
                    Severity::Warning,
                    "link-target",
                    message,
                    link.target.clone(),
                    Some("delete that passage, or use <<link>> with <<goto>>".into()),
                );
            }
            if let Some((setter_at, setter)) = &link.setter {
                let lower = link.target.to_ascii_lowercase();
                if lower.contains("://") || lower.starts_with("mailto:") {
                    let message = format!("[[…|{}][…]] has a setter, and setters only work on links to passages", link.target);
                    at(out, passage, link.start, Severity::Warning, "link-target", message, link.target.clone(), None);
                } else if let Some(error) = js::statements(setter) {
                    let message = format!("The link's setter doesn't parse as JavaScript: {}", error.message);
                    at(out, passage, setter_at + error.offset, Severity::Error, "js-syntax", message, "[[…][setter]]".into(), None);
                }
            }
        }

        // Code in HTML attributes: @attr="expression", sc-eval:attr, data-setter="statements".
        for (offset, attribute, code, statements) in attribute_code(markup) {
            if attribute == "@data-setter" {
                let message = "data-setter can't be evaluated with @: SugarCube throws".to_string();
                at(out, passage, offset, Severity::Error, "macro-args", message, attribute.clone(), Some("write data-setter=\"…\" without the @".into()));
                continue;
            }
            let error = if statements { js::statements(&code) } else { js::expression(&code) };
            if let Some(error) = error {
                let message = format!("{attribute}'s code doesn't parse as JavaScript: {}", error.message);
                at(out, passage, offset, Severity::Error, "js-syntax", message, attribute.clone(), None);
            }
        }

        // Harlowe in a SugarCube story: it shows as text.
        if let Some((offset, call)) = harlowe_call(&prose) {
            let message = format!("({call}:) is a Harlowe macro, and this is a SugarCube story, so it shows as text");
            at(out, passage, offset, Severity::Error, "wrong-format", message, format!("({call}:)"), Some("write it as a SugarCube macro: <<…>>".into()));
        }
    }
}

/// Text with the insides of its strings blanked.
fn outside_strings(text: &str) -> String {
    let mut out = text.as_bytes().to_vec();
    for (offset, value, _) in scan::strings(text) {
        for byte in &mut out[offset..offset + value.len()] {
            *byte = b' ';
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn props_closest(id: &str, tracks: &HashSet<String>, groups: &HashSet<String>) -> Option<String> {
    let all: Vec<String> = tracks.iter().chain(groups.iter()).cloned().collect();
    crate::props::closest(id, &all).map(|n| format!("did you mean \"{n}\"?"))
}

/// A lone `=` (after `to` is read as one) outside strings: not ==, ===, !=, <=, >= or =>.
fn lone_assignment(args: &str) -> Option<usize> {
    let (js, map) = js::desugar(args);
    let bytes = js.as_bytes();
    let mut quote: Option<u8> = None;
    for i in 0..bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q && (i == 0 || bytes[i - 1] != b'\\') => quote = None,
            Some(_) => {}
            None if matches!(b, b'"' | b'\'' | b'`') => quote = Some(b),
            None if b == b'=' => {
                let before = if i > 0 { bytes[i - 1] } else { b' ' };
                let after = bytes.get(i + 1).copied().unwrap_or(b' ');
                if !matches!(before, b'=' | b'!' | b'<' | b'>' | b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'?') && !matches!(after, b'=' | b'>') {
                    return Some(js::map_back_public(&map, i).min(args.len()));
                }
            }
            None => {}
        }
    }
    None
}

/// Whether the arguments have a comma between two of them, outside strings and brackets: `"Go", "Next"`.
fn comma_between_args(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let (mut quote, mut depth) = (None::<u8>, 0i32);
    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' | b'`' => quote = Some(b),
                b'[' | b'(' | b'{' => depth += 1,
                b']' | b')' | b'}' => depth -= 1,
                b',' if depth == 0 => {
                    let before = raw[..i].trim_end();
                    let after = raw[i + 1..].trim_start();
                    return before.ends_with(['"', '\'', '`', ']']) && after.starts_with(['"', '\'', '`', '[', '$', '_']);
                }
                _ => {}
            },
        }
    }
    false
}

/// `$1st` and `$my-var` in code: not story variables.
fn bad_variable_names(args: &str) -> Vec<(usize, String, &'static str)> {
    let bytes = args.as_bytes();
    let mut out = vec![];
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if matches!(b, b'"' | b'\'' | b'`') => quote = Some(b),
            None if b == b'$' && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')) => {
                if bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                    let end = (i + 1..bytes.len()).find(|&j| !(bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')).unwrap_or(bytes.len());
                    out.push((i, args[i..end].to_string(), "it starts with a digit"));
                    i = end;
                    continue;
                }
                let end = (i + 1..bytes.len()).find(|&j| !(bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')).unwrap_or(bytes.len());
                // $my-var: a hyphen straight between two words, as a name, is a subtraction.
                if end > i + 1
                    && bytes.get(end) == Some(&b'-')
                    && bytes.get(end + 1).is_some_and(|c| c.is_ascii_alphabetic())
                    && (end + 1..bytes.len()).find(|&j| !(bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')).is_none_or(|j| bytes[j] != b'(')
                {
                    let tail = (end + 1..bytes.len()).find(|&j| !(bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')).unwrap_or(bytes.len());
                    out.push((i, args[i..tail].to_string(), "a hyphen in it subtracts"));
                    i = tail;
                    continue;
                }
                i = end;
                continue;
            }
            None => {}
        }
        i += 1;
    }
    out
}

/// `def`/`ndef` beside `and`/`or` without parentheses: `def $x and $x gt 1`.
fn loose_def(args: &str) -> Option<usize> {
    let words: Vec<(usize, &str)> = args.split_whitespace().map(|w| (w.as_ptr() as usize - args.as_ptr() as usize, w)).collect();
    let logic = words.iter().any(|(_, w)| matches!(*w, "and" | "or" | "&&" | "||"));
    if !logic {
        return None;
    }
    words.iter().find(|(at, w)| matches!(*w, "def" | "ndef") && !args[..*at].trim_end().ends_with('(')).map(|(at, _)| *at)
}

/// The variables a `<<for>>` or `<<capture>>` names: `_i`, `_i, _v range $list`, `_i to 0; _i lt 3; _i++`.
fn loop_variables(args: &str) -> Vec<String> {
    let head = args.split(" range ").next().unwrap_or(args).split(';').next().unwrap_or(args);
    let mut out: Vec<String> = vec![];
    for part in head.split([',', ' ']) {
        let part = part.trim();
        if part.starts_with(['_', '$']) && part.len() > 1 && part[1..].chars().all(|c| c.is_alphanumeric() || c == '_') && !out.contains(&part.to_string()) {
            out.push(part.to_string());
        }
    }
    out
}

/// The index of the tag that closes the container at `open`, or the end.
fn matching_close(tags: &[MacroTag], open: usize) -> usize {
    let name = &tags[open].name;
    let mut depth = 0;
    for (index, tag) in tags.iter().enumerate().skip(open) {
        if &tag.name == name {
            if tag.closing {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            } else {
                depth += 1;
            }
        }
    }
    tags.len()
}

/// Whether text uses a variable by name (`_i` but not `_index`).
fn uses_name(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| !text[at + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

/// Naked variables with more than naked variable markup takes: a method call, or an expression in brackets.
fn naked_expressions(prose: &str) -> Vec<(usize, String)> {
    let bytes = prose.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let starts = matches!(bytes[i], b'$' | b'_')
            && bytes.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic())
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'));
        // A bare _word in prose is too often just text: only $ for naked variables.
        if !starts || bytes[i] == b'_' {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        loop {
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if bytes.get(j) == Some(&b'.') && bytes.get(j + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
                j += 1;
                continue;
            }
            if bytes.get(j) == Some(&b'(') && prose[i..j].contains('.') {
                let close = prose[j..].find(')').map(|e| j + e + 1).unwrap_or(j + 1);
                out.push((i, prose[i..close].to_string()));
                break;
            }
            if bytes.get(j) == Some(&b'[') {
                let close = prose[j..].find(']').map(|e| j + e).unwrap_or(j);
                let inside = &prose[j + 1..close];
                let simple = inside.trim().parse::<f64>().is_ok()
                    || inside.starts_with(['"', '\''])
                    || inside.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$');
                if !simple {
                    out.push((i, prose[i..=close].to_string()));
                    break;
                }
                j = close + 1;
                continue;
            }
            break;
        }
        i = j.max(i + 1);
    }
    out
}

/// Code in HTML attributes: (offset, attribute, code, whether it's statements). `@src="…"` and `sc-eval:src="…"` are
/// expressions; `data-setter="…"` holds statements.
fn attribute_code(markup: &str) -> Vec<(usize, String, String, bool)> {
    let mut out = vec![];
    for (offset, value, _) in scan::strings(markup) {
        let head = markup[..offset.saturating_sub(1)].trim_end();
        let Some(head) = head.strip_suffix('=') else { continue };
        let attribute: String = head
            .trim_end()
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '@' | ':' | '_'))
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        // Only in an HTML tag: the attribute follows `<tag ` and a space.
        let tag_open = markup[..offset]
            .rfind('<')
            .is_some_and(|lt| !markup[lt..offset].contains('>') && markup.as_bytes().get(lt + 1).is_some_and(|c| c.is_ascii_alphabetic()));
        if !tag_open {
            continue;
        }
        if attribute == "@data-setter" {
            out.push((offset, attribute, value, true));
        } else if attribute.starts_with('@') || attribute.starts_with("sc-eval:") {
            out.push((offset, attribute, value, false));
        } else if attribute == "data-setter" {
            out.push((offset, attribute, value, true));
        }
    }
    out
}

/// A Harlowe macro call in text: `(set: $a to 1)`, `(if: $x)[…]`.
fn harlowe_call(prose: &str) -> Option<(usize, String)> {
    for (at, _) in prose.match_indices('(') {
        let rest = &prose[at + 1..];
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        if !name.is_empty() && rest[name.len()..].starts_with(':') && HARLOWE.contains(&name.to_ascii_lowercase().as_str()) {
            // `(see: the note)` in prose: only names Harlowe has, followed by `:` and then code-like text.
            let after = rest[name.len() + 1..].trim_start();
            if after.starts_with(['$', '_', '"', '\'', '?', '(']) || after.chars().next().is_some_and(|c| c.is_ascii_digit()) || name == "else" {
                return Some((at, name));
            }
        }
    }
    None
}
