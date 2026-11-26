//! The media a story names: images, audio and video, in passages, the story's JavaScript and its stylesheet.

use std::path::{Path, PathBuf};

use crate::Story;
use serde::Serialize;

use crate::markup::{self as scan, Keep};

pub const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "bmp", "svg"];
pub const OTHER_EXTS: &[&str] = &["mp3", "ogg", "oga", "wav", "m4a", "aac", "flac", "opus", "weba", "mp4", "webm", "ogv", "mov", "m4v"];

pub fn is_media(path: &str) -> bool {
    ext_of(path).is_some_and(|e| IMAGE_EXTS.contains(&e.as_str()) || OTHER_EXTS.contains(&e.as_str()))
}

fn ext_of(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?;
    let name = name.split(['?', '#']).next()?;
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())
}

/// Where a reference is written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Place {
    Passage(String),
    Script(usize),
    Stylesheet(usize),
}

/// One media path the story writes.
#[derive(Debug, Clone, Serialize)]
pub struct MediaRef {
    pub place: Place,
    pub file: PathBuf,
    pub line: usize,
    /// The path as written, which a rewrite replaces.
    pub written: String,
    /// Its offset in the passage's or the code's text.
    pub offset: usize,
    /// The file it names, relative to the story's root, or None for a URL or data.
    pub path: Option<String>,
    /// True when it's built at runtime, so only `written`, a prefix or suffix, is known.
    pub dynamic: bool,
    /// True where only a path can stand: `src=`, `[img[...]]`, `url()`. A string in code that looks like a file name may be
    /// only words the story shows.
    pub certain: bool,
}

/// Every media reference in the story.
pub fn refs(story: &Story) -> Vec<MediaRef> {
    let mut out = vec![];
    for passage in &story.passages {
        if passage.has_tag("Twine.image") {
            continue;
        }
        let place = Place::Passage(passage.name.clone());
        let line = |offset: usize| passage.line_at(offset);
        scan_text(&passage.text, &place, &passage.file, &line, &mut out);
    }
    for (at, code) in story.scripts.iter().enumerate() {
        let line = |offset: usize| code.line + code.text[..offset].matches('\n').count();
        scan_text(&code.text, &Place::Script(at), &code.file, &line, &mut out);
    }
    for (at, code) in story.stylesheets.iter().enumerate() {
        let line = |offset: usize| code.line + code.text[..offset].matches('\n').count();
        scan_text(&code.text, &Place::Stylesheet(at), &code.file, &line, &mut out);
    }
    out.sort_by(|a, b| (format!("{:?}", a.place), a.offset).cmp(&(format!("{:?}", b.place), b.offset)));
    out.dedup_by(|a, b| a.place == b.place && a.offset == b.offset);
    out
}

fn scan_text(text: &str, place: &Place, file: &Path, line: &dyn Fn(usize) -> usize, out: &mut Vec<MediaRef>) {
    let code = scan::blank(text, Keep::Code);
    let markup = scan::blank(text, Keep::Markup);
    let mut push = |offset: usize, written: &str, dynamic: bool, certain: bool| {
        let trimmed = written.trim();
        if trimmed.is_empty() {
            return;
        }
        let lead = written.len() - written.trim_start().len();
        out.push(MediaRef {
            place: place.clone(),
            file: file.to_path_buf(),
            line: line(offset),
            written: trimmed.to_string(),
            offset: offset + lead,
            path: if dynamic { None } else { normalise(trimmed) },
            dynamic,
            certain,
        });
    };
    // [img[...]] images.
    let (_, images) = scan::links(&markup);
    for image in images {
        let dynamic = image.src.starts_with(['$', '_']) || image.src.contains(['(', '+']);
        push(image.src_start + (markup[image.src_start..].len() - markup[image.src_start..].trim_start().len()), &image.src, dynamic, true);
    }
    // CSS url(...), quoted or not.
    let lower = code.to_ascii_lowercase();
    let mut at = 0;
    while let Some(i) = lower[at..].find("url(") {
        let start = at + i + 4;
        let end = code[start..].find(')').map(|e| start + e).unwrap_or(code.len());
        let raw = &code[start..end];
        let inner = raw.trim().trim_matches(['"', '\'']);
        let offset = start + raw.find(inner).unwrap_or(0);
        if !inner.is_empty() && !inner.contains(['$', '+']) {
            push(offset, inner, false, true);
        }
        at = end;
    }
    // HTML attributes left unquoted: src=a.png.
    for name in ["src=", "poster="] {
        let mut at = 0;
        while let Some(i) = lower[at..].find(name) {
            let start = at + i + name.len();
            at = start;
            let preceded = lower[..at - name.len()].chars().last().is_some_and(|c| c.is_ascii_whitespace());
            if !preceded || matches!(code.as_bytes().get(start), Some(b'"' | b'\'') | None) {
                continue;
            }
            let end = code[start..].find(|c: char| c.is_ascii_whitespace() || c == '>').map(|e| start + e).unwrap_or(code.len());
            let value = &code[start..end];
            if is_media(value) {
                push(start, value, false, true);
            }
        }
    }
    // Every quoted string that names a media file, in markup or code: src="…", "…" in JS, <<cacheaudio "id" "…">>.
    // A string that ends at a `+`, or starts after one, is part of a path built at runtime.
    // A string can hold strings of its own: @src="'media/' + $n + '.gif'", or HTML in JavaScript, '<img src="a.png">'.
    let found = scan::all_strings(&code);
    for (offset, value, after) in found {
        let value_trim = value.trim();
        let joined_after = code[after..].trim_start().starts_with('+');
        let joined_before = code[..offset - 1].trim_end().ends_with('+');
        let format_prefixed = value_trim.split_once('|').map(|(_, p)| p).filter(|p| is_media(p));
        // In an attribute that only takes a path: src="…", poster="…", href="…".
        let head = code[..offset.saturating_sub(1)].trim_end();
        let attribute =
            head.strip_suffix('=').map(|h| h.trim_end().rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).next().unwrap_or("").to_ascii_lowercase());
        let certain = attribute.is_some_and(|a| matches!(a.as_str(), "src" | "poster" | "href" | "data-src"));
        let only_extension = value_trim.starts_with('.') && !value_trim[1..].contains(['.', '/']);
        // A path with markup in it is built as the story runs: "pics/<<print $n>>.webm", "pics/$n.png", "pics/{{n}}.png".
        let markup_at = ["<<", "$", "{{", "_args"].iter().filter_map(|m| value_trim.find(m)).min();
        if let Some(at) = markup_at.filter(|_| is_media(value_trim) || value_trim.contains('/')) {
            let prefix = &value_trim[..at];
            if looks_like_folder(prefix) {
                push(offset + value.find(prefix).unwrap_or(0), prefix, true, false);
            }
            continue;
        }
        if let Some(path) = format_prefixed {
            push(offset + value.find(path).unwrap_or(0), path, false, true);
        } else if is_media(value_trim) && !only_extension && !joined_after && !joined_before && !value_trim.contains('\n') {
            push(offset, &value, false, certain);
        } else if joined_after && looks_like_folder(value_trim) {
            push(offset, &value, true, false);
        } else if joined_after {
            // A path that starts inside HTML in the string: '<img src="assets/rooms/room' + n + '.webp">'.
            let cut = value.rfind(['"', '\'', '`', '(', '=']).map(|at| at + 1).unwrap_or(0);
            let tail = &value[cut..];
            if cut > 0 && looks_like_folder(tail.trim()) {
                push(offset + cut, tail, true, false);
            }
        } else if joined_before && only_extension && is_media(&format!("x{value_trim}")) {
            push(offset, &value, true, false);
        }
    }
}

fn looks_like_folder(value: &str) -> bool {
    value.contains('/') && !value.contains(['<', '>'])
}

/// A written path as a path in the story's folder: no URLs or data, no `./`, `%20` decoded, `\` as `/`.
pub fn normalise(written: &str) -> Option<String> {
    let lower = written.to_ascii_lowercase();
    if lower.starts_with("data:") || lower.contains("://") || lower.starts_with("//") || lower.starts_with('#') {
        return None;
    }
    let path = written.split(['?', '#']).next()?.replace('\\', "/");
    let path = percent_decode(&path);
    // `a/../b` is `b`, and `./a` is `a`; a path that climbs out of the story's folder stays as written.
    let mut parts: Vec<&str> = vec![];
    for part in path.split('/') {
        match part {
            "." | "" if !parts.is_empty() || part == "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    Some(if path.ends_with('/') && !joined.is_empty() { format!("{joined}/") } else { joined })
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Two hex digits after a %; checked as bytes, so a % before a multi-byte character can't split it.
        if bytes[i] == b'%' && i + 2 < bytes.len() && bytes[i + 1].is_ascii_hexdigit() && bytes[i + 2].is_ascii_hexdigit() {
            let hex = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hex(bytes[i + 1]) * 16 + hex(bytes[i + 2]));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The media files in the story's folder, as paths relative to it, and those its mounts will put there.
pub fn files(story: &Story) -> Vec<String> {
    let mut out = vec![];
    walk(&story.media_root, &story.media_root, &mut out);
    for (source, at) in &story.media_mounts {
        if source.is_dir() {
            let mut inside = vec![];
            walk(source, source, &mut inside);
            out.extend(inside.into_iter().map(|f| format!("{at}/{f}")));
        } else if source.is_file() && is_media(at) {
            out.push(at.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Where a path relative to the media root is on disk, with mounts: `media/sea.png` under a mount of `src/media` at
/// `media` is `src/media/sea.png`.
pub fn path_of(root: &Path, mounts: &[(PathBuf, String)], rel: &str) -> PathBuf {
    for (source, at) in mounts {
        if rel == at {
            return source.clone();
        }
        if let Some(rest) = rel.strip_prefix(at.as_str()).and_then(|r| r.strip_prefix('/')) {
            return source.join(rest);
        }
    }
    root.join(rel)
}

pub use crate::skipped_folder;

/// A path only the author's computer has: file://, a drive letter, or one from the root.
pub fn is_absolute_path(written: &str) -> bool {
    let lower = written.trim().to_ascii_lowercase();
    let bytes = lower.as_bytes();
    lower.starts_with("file:")
        || (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
        || (lower.starts_with('/') && !lower.starts_with("//"))
        || lower.starts_with("\\\\")
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Never a hidden file, nor anything in a skipped folder.
        let hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(crate::passed_by);
        if hidden {
            continue;
        }
        // The listing's own file type, not a stat per file (slow on a Windows drive under WSL).
        let is_dir = entry.file_type().map(|t| t.is_dir() || (t.is_symlink() && path.is_dir())).unwrap_or(false);
        if is_dir {
            walk(root, &path, out);
        } else if let Some(rel) = path.strip_prefix(root).ok().and_then(|p| p.to_str()) {
            let rel = rel.replace('\\', "/");
            if is_media(&rel) {
                out.push(rel);
            }
        }
    }
}

/// Folders the story's code names on their own, such as `var base = "Images/"`, which it puts in front of other paths.
pub fn bases(story: &Story) -> Vec<String> {
    let mut texts: Vec<&str> = story.scripts.iter().map(|c| c.text.as_str()).collect();
    texts.extend(story.passages.iter().map(|p| p.text.as_str()));
    let mut out: Vec<String> = vec![];
    for text in texts {
        for (_, value, _) in scan::strings(&scan::blank(text, Keep::Code)) {
            let value = value.trim();
            if value.ends_with('/')
                && value.len() > 1
                && !value.contains("://")
                && story.media_path(value.trim_end_matches('/')).is_dir()
                && let Some(base) = normalise(value)
            {
                out.push(base);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The story's media: what it names, what's in its folder, and the folders its code puts in front of paths.
pub struct Media {
    pub refs: Vec<MediaRef>,
    pub files: Vec<String>,
    pub bases: Vec<String>,
    /// The folder the files are in, and every path is relative to: the story's `media_root`.
    pub root: PathBuf,
    /// The story's mounts: files that will be under `root` once it's built.
    mounts: Vec<(PathBuf, String)>,
    lower: std::collections::HashMap<String, String>,
    set: std::collections::HashSet<String>,
    /// The story's image passages (`Twine.image`), which `[img[name]]` names instead of a file.
    images: std::collections::HashSet<String>,
}

/// Which files the story uses.
#[derive(Debug, Default)]
pub struct Usage {
    /// The files references name (written with other capitals, too).
    pub used: std::collections::HashSet<String>,
    /// The files nothing names, and that aren't in a folder the story builds paths in.
    pub unused: Vec<String>,
    /// The folders the story builds paths in as it runs (`"Images/Day " + $n`): every file in one might be used.
    pub prefixes: Vec<String>,
}

/// The file a reference names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub file: String,
    /// The folder the story's code puts in front of the path, when it needed one.
    pub base: Option<String>,
    /// True when only the capitals differ, which works on Windows and macOS and not on most web hosts.
    pub case_differs: bool,
}

impl Media {
    pub fn of(story: &Story) -> Media {
        let files = files(story);
        let lower = files.iter().map(|f| (f.to_lowercase(), f.clone())).collect();
        let set = files.iter().cloned().collect();
        let images = story.passages.iter().filter(|p| p.has_tag("Twine.image")).map(|p| p.name.clone()).collect();
        Media { refs: refs(story), bases: bases(story), files, lower, set, root: story.media_root.clone(), mounts: story.media_mounts.clone(), images }
    }

    pub fn resolve(&self, r: &MediaRef) -> Option<Resolved> {
        let path = r.path.as_ref()?;
        let candidates = std::iter::once((None, path.clone())).chain(self.bases.iter().map(|b| (Some(b.clone()), format!("{b}{path}"))));
        let candidates: Vec<_> = candidates.collect();
        for (base, full) in &candidates {
            if self.set.contains(full) {
                return Some(Resolved { file: full.clone(), base: base.clone(), case_differs: false });
            }
        }
        for (base, full) in &candidates {
            if let Some(file) = self.lower.get(&full.to_lowercase()) {
                return Some(Resolved { file: file.clone(), base: base.clone(), case_differs: true });
            }
        }
        None
    }

    /// Where one of `files` is on disk.
    pub fn path_of(&self, rel: &str) -> PathBuf {
        path_of(&self.root, &self.mounts, rel)
    }

    /// Which files the story uses. A path from the author's own computer and the name of an image passage name no file.
    pub fn usage(&self) -> Usage {
        let mut used = std::collections::HashSet::new();
        for r in &self.refs {
            if !r.dynamic && is_absolute_path(&r.written) {
                continue;
            }
            let Some(path) = &r.path else { continue };
            if self.images.contains(path) {
                continue;
            }
            if let Some(found) = self.resolve(r) {
                used.insert(found.file);
            }
        }
        let prefixes = self.prefixes();
        let unused = self.files.iter().filter(|f| !used.contains(*f) && !prefixes.iter().any(|p| f.starts_with(p.as_str()))).cloned().collect();
        Usage { used, unused, prefixes }
    }

    /// Paths the story builds as it runs: the folders they start in.
    pub fn prefixes(&self) -> Vec<String> {
        // Only a prefix whose folder is really there: "Images/Day " is one; "", "/" and "</strong> /" aren't.
        let real = |prefix: &String| {
            let folder = prefix.rfind('/').map(|at| &prefix[..at]).unwrap_or("");
            !folder.is_empty() && !prefix.contains(['<', '>']) && self.path_of(folder).is_dir()
        };
        let mut out: Vec<String> =
            self.refs.iter().filter(|r| r.dynamic && !r.written.starts_with('.')).filter_map(|r| normalise(&r.written)).filter(real).collect();
        out.sort();
        out.dedup();
        out
    }
}

/// Whether a file exists under the story's root, matching case as a case-sensitive web host would.
pub fn exists(story: &Story, path: &str) -> bool {
    story.media_path(path).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story(text: &str) -> Story {
        let mut story = Story::read(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/small/story.twee"))).unwrap();
        story.passages = crate::parse_twee(&format!(":: Test\n{text}\n"), Path::new("t.twee"));
        story.scripts.clear();
        story.stylesheets.clear();
        story
    }

    #[test]
    fn finds_media_in_every_form() {
        let s = story(
            r#"[img[pics/a.png]] <img src="pics/b c.jpg"> <img src=pics/d.gif> <<cacheaudio "theme" "mp3|music/t.mp3">> <div style="background: url('bg.webp')"></div> <<set $p to "Images/x/" + $n + ".gif">> <a href="https://x.com/y.png">"#,
        );
        let found: Vec<_> = refs(&s).into_iter().map(|r| (r.written, r.dynamic)).collect();
        assert!(found.contains(&("pics/a.png".into(), false)), "{found:?}");
        assert!(found.contains(&("pics/b c.jpg".into(), false)), "{found:?}");
        assert!(found.contains(&("pics/d.gif".into(), false)), "{found:?}");
        assert!(found.contains(&("music/t.mp3".into(), false)), "{found:?}");
        assert!(found.contains(&("bg.webp".into(), false)), "{found:?}");
        assert!(found.contains(&("Images/x/".into(), true)), "{found:?}");
        assert!(found.contains(&(".gif".into(), true)), "{found:?}");
        assert_eq!(refs(&s).iter().filter(|r| r.path.is_none() && !r.dynamic).count(), 1);
    }

    #[test]
    fn normalises_paths() {
        assert_eq!(normalise("./Images/Day%201/a.gif?v=2").as_deref(), Some("Images/Day 1/a.gif"));
        assert_eq!(normalise("data:image/png;base64,xx"), None);
        assert_eq!(normalise("media/rooms/../shop.png").as_deref(), Some("media/shop.png"));
        assert_eq!(normalise("../up.png").as_deref(), Some("../up.png"));
        assert_eq!(normalise("./Images/").as_deref(), Some("Images/"));
        assert_eq!(normalise("a%2é.png").as_deref(), Some("a%2é.png"));
    }
}

#[cfg(test)]
mod usage_tests {
    use super::*;

    /// A story in a fresh folder with these files beside it.
    fn story_with(files: &[&str], text: &str) -> (Story, PathBuf) {
        let dir = std::env::temp_dir().join(format!("twee-tools-usage-{}-{}", files.len(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in files {
            std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
            std::fs::write(dir.join(file), "x").unwrap();
        }
        let story = Story::from_twee(text, &dir.join("story.twee")).unwrap();
        (story, dir)
    }

    #[test]
    fn used_unused_and_folders_built_at_runtime() {
        let (story, dir) = story_with(
            &["media/sea.png", "media/Sky.png", "media/spare.png", "days/day1.jpg", "days/day2.jpg"],
            ":: Start\n<img src=\"media/sea.png\"> [img[media/sky.png]] <img src=\"C:/Users/me/media/spare.png\">\n<<set _p to \"days/day\" + $n + \".jpg\">>\n",
        );
        let usage = Media::of(&story).usage();
        let mut used: Vec<&str> = usage.used.iter().map(String::as_str).collect();
        used.sort();
        assert_eq!(used, ["media/Sky.png", "media/sea.png"], "other capitals still use the file");
        assert_eq!(usage.unused, ["media/spare.png"], "an absolute path uses nothing; a built folder's files may be used");
        assert_eq!(usage.prefixes, ["days/day"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_image_passage_name_isnt_a_file() {
        // [img[bg.png]] shows the image passage, so the file of the same name is still unused.
        let (story, dir) = story_with(&["bg.png"], ":: Start\n[img[bg.png]]\n\n:: bg.png [Twine.image]\ndata:image/png;base64,AA==\n");
        assert_eq!(Media::of(&story).usage().unused, ["bg.png"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
