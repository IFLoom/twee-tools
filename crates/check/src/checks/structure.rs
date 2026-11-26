//! Passages declared twice, and passages left unwritten.

use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::*;

/// Passages declared more than once. The story has kept the last of each (as the build ships it), so every other check
/// reads that one; this says where the others were.
pub(crate) fn duplicates(ix: &Index, _config: &Config, out: &mut Out) {
    for duplicate in &ix.story.duplicates {
        let message = format!("\"{}\" is declared twice", duplicate.name);
        let first = duplicate.first_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        out.in_file(Severity::Error, "duplicate-passage", duplicate.file.clone(), duplicate.line, Some(duplicate.name.clone()), message)
            .about(duplicate.name.clone())
            .hint(Some(format!("the first is at {first}:{}; the build uses this later one", duplicate.first_line)));
    }
}

/// Passages with nothing in them, or only the text Twine puts in a new one.
pub(crate) fn unwritten(ix: &Index, _config: &Config, out: &mut Out) {
    for (_, passage, _parse) in ix.passages() {
        let special = ix.format.is_special(passage);
        if special || !is_unwritten(passage) {
            continue;
        }
        let what = if passage.text.trim().is_empty() { "is empty" } else { "still has Twine's placeholder text, \"Double-click this passage to edit it.\"" };
        out.in_file(Severity::Warning, "unwritten", passage.file.clone(), passage.line, Some(passage.name.clone()), format!("\"{}\" {what}", passage.name))
            .about(passage.name.clone());
    }
}
