//! JavaScript that doesn't parse: the story's own, and the expressions in its macros.

use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::*;

/// Syntax errors in the story's JavaScript, <<script>> and <script> bodies, and the expressions macros take.
pub(crate) fn javascript(ix: &Index, _config: &Config, out: &mut Out) {
    for code in &ix.story.scripts {
        if let Some(error) = js::script(&code.text) {
            let line = code.line + code.text[..error.offset.min(code.text.len())].matches('\n').count();
            let message = format!("The story's JavaScript doesn't parse: {}", error.message);
            out.in_file(Severity::Error, "js-syntax", code.file.clone(), line, None, message).about("Story JavaScript");
        }
    }
    if !ix.format.sugarcube() {
        return;
    }
    // <<set>> and <<run>> run statements (`<<set $a to 1; $b to 2>>`); the rest take one expression.
    const STATEMENTS: &[&str] = &["set", "run"];
    const EXPRESSIONS: &[&str] = &["if", "elseif", "print", "=", "-", "switch"];
    for (_, passage, parse) in ix.passages() {
        let _markup = &parse.markup;
        let tags = &parse.tags;
        let mut found: Vec<(usize, String, String)> = vec![];
        for (at, tag) in tags.iter().enumerate() {
            if tag.closing {
                continue;
            }
            let statements = STATEMENTS.contains(&tag.name.as_str());
            if (statements || EXPRESSIONS.contains(&tag.name.as_str())) && !tag.args.trim().is_empty() {
                let error = if statements { js::statements(&tag.args) } else { js::expression(&tag.args) };
                if let Some(error) = error {
                    found.push((tag.args_start + error.offset, format!("<<{}>>", tag.name), error.message));
                }
            } else if tag.name == "script" {
                // The body is blanked in markup; take it from the text, to the closing tag.
                let end = tags[at..].iter().find(|t| t.closing && t.name == "script").map(|t| t.start).unwrap_or(passage.text.len());
                let body = &passage.text[tag.end.min(end)..end];
                let twine = tag.args.contains("TwineScript");
                let error = if twine { js::script_twine(body) } else { js::script(body) };
                if let Some(error) = error {
                    found.push((tag.end + error.offset, "<<script>>".into(), error.message));
                }
            }
        }
        // <script> in a passage's HTML.
        let lower = passage.text.to_ascii_lowercase();
        let mut from = 0;
        while let Some(open) = lower[from..].find("<script").map(|i| from + i) {
            // <<script>> is a macro, handled above.
            if open > 0 && lower.as_bytes()[open - 1] == b'<' {
                from = open + 1;
                continue;
            }
            let Some(body_start) = lower[open..].find('>').map(|i| open + i + 1) else {
                break;
            };
            let end = lower[body_start..].find("</script>").map(|i| body_start + i).unwrap_or(passage.text.len());
            let opener = &lower[open..body_start];
            let javascript = !opener.contains("type=") || opener.contains("javascript") || opener.contains("module");
            if javascript && let Some(error) = js::script(&passage.text[body_start..end]) {
                found.push((body_start + error.offset, "<script>".into(), error.message));
            }
            from = end.max(open + 1);
        }
        for (offset, subject, message) in found {
            let message = format!("{subject} doesn't parse as JavaScript: {message}");
            out.at(Severity::Error, "js-syntax", passage, offset, message).subject = Some(subject);
        }
    }
}
