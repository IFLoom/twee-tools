//! What the checks report into: findings, made only by the constructors here, the counts and the media summary.

use std::path::PathBuf;

use story::{Passage, Story};

use crate::{Finding, MediaSummary, Severity, Stats};

#[derive(Default)]
pub(crate) struct Out {
    findings: Vec<Finding>,
    pub(crate) stats: Stats,
    pub(crate) summary: MediaSummary,
}

impl Out {
    /// A finding in a passage, at an offset in its text.
    pub(crate) fn at(&mut self, severity: Severity, code: &'static str, passage: &Passage, offset: usize, message: String) -> &mut Finding {
        self.in_file(severity, code, passage.file.clone(), passage.line_at(offset), Some(passage.name.clone()), message)
    }

    /// A finding about the story as a whole, at its StoryData (or its first file, without one).
    pub(crate) fn story_data(&mut self, story: &Story, severity: Severity, code: &'static str, message: String) -> &mut Finding {
        let (file, line) =
            story.data_at.clone().unwrap_or_else(|| (story.files.first().or(story.passages.first().map(|p| &p.file)).cloned().unwrap_or_default(), 1));
        self.in_file(severity, code, file, line, None, message)
    }

    /// A finding at a line of a file: a script, a definition, a media reference, a media file.
    pub(crate) fn in_file(
        &mut self,
        severity: Severity,
        code: &'static str,
        file: PathBuf,
        line: usize,
        passage: Option<String>,
        message: String,
    ) -> &mut Finding {
        self.findings.push(Finding { severity, code, message, file, line, passage, subject: None, hint: None, bytes: None });
        self.findings.last_mut().unwrap()
    }

    /// Adds what another out holds to this one: its findings after these, its counts to these. Each count and the media
    /// summary come from one check, so adding them is the same as that check having written here.
    pub(crate) fn absorb(&mut self, other: Out) {
        self.findings.extend(other.findings);
        let (s, o) = (&mut self.stats, other.stats);
        s.passages += o.passages;
        s.words += o.words;
        s.links += o.links;
        s.dynamic_links += o.dynamic_links;
        s.reached_by_pattern += o.reached_by_pattern;
        s.variables += o.variables;
        s.macros += o.macros;
        s.media_refs += o.media_refs;
        s.dynamic_media += o.dynamic_media;
        s.media_files += o.media_files;
        let (m, o) = (&mut self.summary, other.summary);
        m.files += o.files;
        m.bytes += o.bytes;
        m.kinds.extend(o.kinds);
        m.compressible_bytes += o.compressible_bytes;
    }

    pub(crate) fn into_parts(self) -> (Vec<Finding>, Stats, MediaSummary) {
        (self.findings, self.stats, self.summary)
    }
}

impl Finding {
    pub(crate) fn about(&mut self, subject: impl Into<String>) -> &mut Finding {
        self.subject = Some(subject.into());
        self
    }

    pub(crate) fn hint(&mut self, hint: Option<String>) -> &mut Finding {
        self.hint = hint;
        self
    }
}
