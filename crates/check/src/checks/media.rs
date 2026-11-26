//! Media: missing, absolute or wrongly capitalised paths, unused and heavy files, and what they weigh.

use std::collections::{BTreeMap, HashSet};

use crate::config::Config;
use crate::index::Index;
use crate::out::Out;
use crate::*;

pub(crate) fn media(ix: &Index, _config: &Config, out: &mut Out, progress: Progress) {
    let media = media::Media::of(ix.story);
    out.stats.media_refs = media.refs.len();
    out.stats.dynamic_media = media.refs.iter().filter(|r| r.dynamic).count();
    out.stats.media_files = media.files.len();
    let images: HashSet<&str> = ix.story.passages.iter().filter(|p| p.has_tag("Twine.image")).map(|p| p.name.as_str()).collect();
    for r in &media.refs {
        // A path from the author's own machine works there and nowhere else (Twine: "use a relative URL").
        if !r.dynamic && media::is_absolute_path(&r.written) {
            let message = format!("\"{}\" is a path on this computer, so it breaks as soon as the story is shared", r.written);
            let hint = relative_hint(&r.written, &media.files);
            out.in_file(Severity::Error, "absolute-media", r.file.clone(), r.line, passage_of(&r.place), message).about(r.written.clone()).hint(hint);
            continue;
        }
        let Some(path) = &r.path else { continue };
        if images.contains(path.as_str()) {
            continue;
        }
        let (severity, message, hint) = match media.resolve(r) {
            Some(found) if !found.case_differs => continue,
            Some(found) => {
                let file = found.file;
                (
                    Severity::Warning,
                    format!("\"{}\" is written with different capitals from the file: it works on Windows and macOS, and not on most web hosts", r.written),
                    Some(format!("the file is \"{file}\"")),
                )
            }
            None if r.certain => (Severity::Error, format!("\"{}\" names a file that isn't there", r.written), elsewhere(path, &media.files)),
            None => (
                Severity::Warning,
                format!("\"{}\" looks like a file, and there's no such file", r.written),
                Some("if it's only words the story shows, ignore this".into()),
            ),
        };
        out.in_file(severity, "missing-media", r.file.clone(), r.line, passage_of(&r.place), message).about(r.written.clone()).hint(hint);
    }
    // Reading each file's first bytes, several at once: most of the time on a big folder is waiting on the disk.
    let total = media.files.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let kinds: Vec<(Option<&'static str>, Option<&'static str>, u64)> = {
        use rayon::prelude::*;
        media
            .files
            .par_iter()
            .map(|file| {
                let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n.is_multiple_of(50) || n == total {
                    progress(&format!("Checking media files: {n} of {total}"));
                }
                sniff(&media.path_of(file))
            })
            .collect()
    };
    let mut by_kind: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for (file, &(_, actual, size)) in media.files.iter().zip(&kinds) {
        let kind = actual.map(str::to_string).unwrap_or_else(|| kind_name(file));
        if matches!(kind.as_str(), "GIF" | "PNG" | "JPEG" | "BMP") {
            out.summary.compressible_bytes += size;
        }
        let entry = by_kind.entry(kind.clone()).or_default();
        entry.0 += 1;
        entry.1 += size;
        out.summary.bytes += size;
        if size >= LARGE_MEDIA {
            let message = format!("\"{file}\" ({kind}) is {}, which every player downloads", megabytes(size));
            out.in_file(Severity::Warning, "large-media", media.path_of(file), 0, None, message).about(file.clone()).hint(Some(kind.clone())).bytes =
                Some(size);
        }
    }
    out.summary.files = media.files.len();
    let mut kinds_sorted: Vec<(String, usize, u64)> = by_kind.into_iter().map(|(k, (n, b))| (k, n, b)).collect();
    kinds_sorted.sort_by_key(|k| std::cmp::Reverse(k.2));
    out.summary.kinds = kinds_sorted;
    for (file, (kind, _, _)) in media.files.iter().zip(kinds) {
        if let Some(actual) = kind {
            let message = format!("\"{file}\" is a {actual} file with another extension: browsers show it, and a strict web host or a packager may not");
            out.in_file(Severity::Warning, "misnamed-media", media.path_of(file), 0, None, message)
                .about(file.clone())
                .hint(Some(format!("it's really a {actual}")));
        }
    }
    for file in &media.usage().unused {
        let message = format!("\"{file}\" is in the story's folder, and nothing uses it");
        out.in_file(Severity::Warning, "unused-media", media.path_of(file), 0, None, message).about(file.clone());
    }
}
