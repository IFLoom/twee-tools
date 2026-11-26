//! Shrinks a story's images: re-encodes JPEG, PNG and GIF (animated too) as WebP, and writes a copy of the story that points
//! at the new files. The originals are never touched.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use story::media as found;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage, GenericImageView, ImageReader, RgbaImage, imageops::FilterType};
use rayon::prelude::*;
use serde::Serialize;
use story::{Form, Story};

#[derive(Debug, Clone)]
pub struct Options {
    /// 0 to 100. Lossy: how close to the original the picture stays (80 is hard to tell apart). Lossless: how hard to
    /// work on the file's size, as libwebp reads it (higher is smaller and slower).
    pub quality: f32,
    /// Keep every pixel exactly: WebP's lossless mode. Bigger than lossy for photos, often smaller than PNG for flat art.
    pub lossless: bool,
    /// How much work to put into each file, 0 (fast) to 6 (smallest): libwebp's "method". 4 by default.
    pub effort: u8,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// Convert files the story can't be rewritten for too, under their old names.
    pub keep_names: bool,
    pub drop_unused: bool,
    pub dry_run: bool,
    /// Files to leave out of the copy, as globs relative to the story's folder: `["**/*.psd", "notes/**"]`. A pattern
    /// with no `/` matches at any depth. The copy is refused if the story uses a file left out.
    pub exclude: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            quality: 80.0,
            lossless: false,
            effort: 4,
            max_width: None,
            max_height: None,
            keep_names: false,
            drop_unused: false,
            dry_run: false,
            exclude: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Re-encoded under a new name, and the story rewritten to it.
    Converted,
    /// Re-encoded under its old name.
    ConvertedInPlace,
    /// Copied unchanged, with the reason.
    Kept(String),
    /// Nothing uses it, and it was left out.
    Dropped,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    pub path: String,
    pub new_path: String,
    pub before: u64,
    pub after: u64,
    pub action: Action,
    pub frames: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub files: Vec<FileResult>,
    pub before: u64,
    pub after: u64,
    /// How many places in the story now name a new file.
    pub rewritten: usize,
    /// Paths built as the story runs, whose files keep their names.
    pub dynamic: Vec<String>,
    pub unused: Vec<String>,
    pub errors: Vec<String>,
    pub out: Option<PathBuf>,
}

impl Report {
    pub fn summary(&self) -> String {
        let mut s = String::new();
        let mb = |b: u64| b as f64 / 1_048_576.0;
        let count = |f: &dyn Fn(&Action) -> bool| self.files.iter().filter(|r| f(&r.action)).count();
        let mut kept: BTreeMap<&str, usize> = BTreeMap::new();
        for r in &self.files {
            if let Action::Kept(why) = &r.action {
                *kept.entry(why.as_str()).or_default() += 1;
            }
        }
        let mut biggest: Vec<&FileResult> = self.files.iter().filter(|r| r.before > r.after).collect();
        biggest.sort_by_key(|r| std::cmp::Reverse(r.before - r.after));
        if !biggest.is_empty() {
            s += "Largest savings:\n";
            for r in biggest.iter().take(10) {
                s += &format!(
                    "  {:>7.1} MB -> {:>6.1} MB  {}{}\n",
                    mb(r.before),
                    mb(r.after),
                    r.path,
                    if r.frames > 1 { format!(" ({} frames)", r.frames) } else { String::new() }
                );
            }
        }
        s += &format!(
            "\n{} files: {} converted and renamed, {} converted under their old names, {} dropped as unused.\n",
            self.files.len(),
            count(&|a| *a == Action::Converted),
            count(&|a| *a == Action::ConvertedInPlace),
            count(&|a| *a == Action::Dropped)
        );
        for (why, n) in kept {
            s += &format!("  {n} kept as they were: {why}\n");
        }
        if !self.dynamic.is_empty() {
            s += &format!(
                "{} paths are built as the story runs, so the files under them keep their names (convert those too with --keep-names): {}\n",
                self.dynamic.len(),
                self.dynamic.join(", ")
            );
        }
        if !self.unused.is_empty() {
            s += &format!(
                "{} files nothing uses{}.\n",
                self.unused.len(),
                if self.files.iter().any(|f| f.action == Action::Dropped) { ", left out" } else { " (leave them out with --drop-unused)" }
            );
        }
        for e in &self.errors {
            s += &format!("  couldn't convert {e}\n");
        }
        let saved = self.before.saturating_sub(self.after);
        s += &format!(
            "Media: {:.1} MB -> {:.1} MB, {:.1} MB saved ({:.0}%). {} places in the story point at new files.\n",
            mb(self.before),
            mb(self.after),
            mb(saved),
            if self.before > 0 { saved as f64 * 100.0 / self.before as f64 } else { 0.0 },
            self.rewritten
        );
        match &self.out {
            Some(out) => s += &format!("Wrote the copy to {}.\n", out.display()),
            None => s += "A dry run: nothing was written.\n",
        }
        s
    }
}

/// One file's encoding: the WebP bytes and the frame count, None when it isn't converted, or why it couldn't be.
type Encoded = Result<Option<(Vec<u8>, usize)>, String>;

/// Runs a closure when dropped: counts an image as done however its encoding ends.
struct Finished<F: Fn()>(F);

impl<F: Fn()> Drop for Finished<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}

/// What `encode` answers for a file that's WebP already, under another name.
pub const ALREADY_WEBP: &str = "it's WebP already";

const CONVERTIBLE: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp"];

fn ext(path: &str) -> String {
    path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// The new name: `a.png` becomes `a.webp`, unless that's taken, then `a.png.webp`. `taken` holds every name in the folder
/// and every name given out so far, in lower case, as Windows and macOS folders ignore case.
fn renamed(path: &str, taken: &mut HashSet<String>) -> String {
    let stem = path.rsplit_once('.').map(|(s, _)| s).unwrap_or(path);
    let mut name = format!("{stem}.webp");
    if taken.contains(&name.to_lowercase()) {
        name = format!("{path}.webp");
    }
    let mut n = 2;
    while taken.contains(&name.to_lowercase()) {
        name = format!("{stem}-{n}.webp");
        n += 1;
    }
    taken.insert(name.to_lowercase());
    name
}

pub fn compress(story: &Story, out: &Path, options: &Options) -> Result<Report, String> {
    compress_with(story, out, options, &|_| {})
}

/// `compress`, saying what it's doing as it goes: "Encoding 120 of 457".
///
/// The copy is one folder: the story's own files where they are beside it, and its media where they are beside the
/// story. So the media must be in the story's folder, as a published page's are.
pub fn compress_with(story: &Story, out: &Path, options: &Options, progress: story::Progress) -> Result<Report, String> {
    if !(0.0..=100.0).contains(&options.quality) {
        return Err(format!("The quality is {}, and it goes from 0 to 100.", options.quality));
    }
    if options.effort > 6 {
        return Err(format!("The effort is {}, and it goes from 0 (fast) to 6 (smallest).", options.effort));
    }
    if story.media_root != story.root {
        return Err(format!(
            "The story's media are in {}, and the story is in {}: compress the page that's beside its media.",
            story.media_root.display(),
            story.root.display()
        ));
    }
    if !options.dry_run {
        let root = fs::canonicalize(&story.root).map_err(|e| e.to_string())?;
        let target = out.canonicalize().unwrap_or_else(|_| out.to_path_buf());
        if target == root || target.starts_with(&root) {
            return Err(format!("Write the copy outside the story's folder, not to {}.", out.display()));
        }
        if out.exists() && fs::read_dir(out).map(|mut d| d.next().is_some()).unwrap_or(false) {
            return Err(format!("{} isn't empty; write the copy to a new folder.", out.display()));
        }
    }
    let media = found::Media::of(story);
    let refs = &media.refs;
    let found::Usage { used, unused, prefixes } = media.usage();
    // What the copy leaves out: never a file the story uses, which would break the copy.
    let leave_out = story::glob_set(&options.exclude).map_err(|e| e.to_string())?;
    let excluded = |rel: &str| !options.exclude.is_empty() && leave_out.is_match(rel);
    if let Some(file) = media.files.iter().find(|f| excluded(f) && used.contains(*f)) {
        return Err(format!("\"{file}\" is left out of the copy by an exclude pattern, and the story uses it; nothing was written."));
    }
    let files: Vec<String> = media.files.iter().filter(|f| !excluded(f)).cloned().collect();
    let files = &files;
    let unused: Vec<String> = unused.into_iter().filter(|f| !excluded(f)).collect();
    let file_set: HashSet<String> = files.iter().cloned().collect();
    let dynamic = prefixes.clone();
    let under_prefix = |f: &str| prefixes.iter().any(|p| f.starts_with(p.as_str()));

    // What to do with each file.
    struct Plan {
        path: String,
        new_path: String,
        rename: bool,
        convert: bool,
        drop: bool,
        reason: String,
    }
    let mut plans = vec![];
    let mut taken: HashSet<String> = file_set.iter().map(|f| f.to_lowercase()).collect();
    for file in files {
        let convertible = CONVERTIBLE.contains(&ext(file).as_str());
        let is_unused = unused.contains(file);
        let renameable = used.contains(file) && !under_prefix(file);
        let (rename, convert, drop, reason) = if is_unused && options.drop_unused {
            (false, false, true, String::new())
        } else if !convertible {
            (false, false, false, format!("{} isn't converted", ext(file)))
        } else if renameable {
            (true, true, false, String::new())
        } else if options.keep_names {
            (false, true, false, String::new())
        } else if is_unused {
            (false, false, false, "nothing uses it".into())
        } else {
            (false, false, false, "its path is built as the story runs".into())
        };
        let new_path = if rename { renamed(file, &mut taken) } else { file.clone() };
        plans.push(Plan { path: file.clone(), new_path, rename, convert, drop, reason });
    }

    // Encode, at most 8 at once, as a long GIF's frames all sit in memory.
    let jobs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(jobs).build().map_err(|e| e.to_string())?;
    let mut order: Vec<usize> = (0..plans.len()).collect();
    // The largest first, so one big GIF doesn't finish alone at the end.
    let size = |p: &str| fs::metadata(media.root.join(p)).map(|m| m.len()).unwrap_or(0);
    order.sort_by_key(|&i| std::cmp::Reverse(size(&plans[i].path)));
    let total = plans.iter().filter(|p| p.convert).count();
    let done = std::sync::atomic::AtomicUsize::new(0);
    progress(&format!("Encoding images: 0 of {total}"));

    let encoded: Vec<(usize, Encoded)> = pool.install(|| {
        order
            .par_iter()
            .map(|&i| {
                let plan = &plans[i];
                if !plan.convert {
                    return (i, Ok(None));
                }
                let _finished = Finished(|| {
                    let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    progress(&format!("Encoding images: {n} of {total}"));
                });
                let bytes = match fs::read(media.root.join(&plan.path)) {
                    Ok(b) => b,
                    Err(e) => return (i, Err(format!("{}: {e}", plan.path))),
                };
                (i, encode(&bytes, &plan.path, options).map(Some).map_err(|e| format!("{}: {e}", plan.path)))
            })
            .collect()
    });
    let mut encoded: HashMap<usize, Encoded> = encoded.into_iter().collect();

    let mut report = Report { dynamic, unused: unused.clone(), out: (!options.dry_run).then(|| out.to_path_buf()), ..Report::default() };
    let mut renames: HashMap<String, String> = HashMap::new();
    let mut writes: Vec<(String, Option<Vec<u8>>)> = vec![];
    for (i, plan) in plans.iter().enumerate() {
        let before = size(&plan.path);
        report.before += before;
        let mut result =
            FileResult { path: plan.path.clone(), new_path: plan.path.clone(), before, after: before, action: Action::Kept(plan.reason.clone()), frames: 0 };
        if plan.drop {
            result.action = Action::Dropped;
            result.after = 0;
        } else {
            match encoded.remove(&i).unwrap_or(Ok(None)) {
                Ok(Some((data, frames))) if (data.len() as u64) < before => {
                    result.after = data.len() as u64;
                    result.frames = frames;
                    if plan.rename {
                        result.new_path = plan.new_path.clone();
                        result.action = Action::Converted;
                        renames.insert(plan.path.clone(), plan.new_path.clone());
                    } else {
                        result.action = Action::ConvertedInPlace;
                    }
                    writes.push((result.new_path.clone(), Some(data)));
                }
                Ok(Some((_, frames))) => {
                    result.frames = frames;
                    result.action = Action::Kept("WebP wasn't smaller".into());
                    writes.push((plan.path.clone(), None));
                }
                Ok(None) => writes.push((plan.path.clone(), None)),
                Err(error) if error.ends_with(ALREADY_WEBP) => {
                    result.action = Action::Kept("it's WebP already, under another name".into());
                    writes.push((plan.path.clone(), None));
                }
                Err(error) => {
                    report.errors.push(error);
                    result.action = Action::Kept("couldn't be read".into());
                    writes.push((plan.path.clone(), None));
                }
            }
        }
        report.after += result.after;
        report.files.push(result);
    }

    // What each written path becomes, by the file it was found in.
    let mut replace: HashMap<PathBuf, Vec<(String, String)>> = HashMap::new();
    for r in refs {
        let Some(found) = media.resolve(r) else {
            continue;
        };
        let Some(new_path) = renames.get(&found.file) else {
            continue;
        };
        let new_written = if found.case_differs {
            // Written with other capitals: write the file's own name, which also fixes it for case-sensitive hosts.
            let base = found.base.as_deref().unwrap_or("");
            new_path.strip_prefix(base).unwrap_or(new_path).to_string()
        } else {
            rewrite_written(&r.written, &found.file, new_path)
        };
        replace.entry(r.file.clone()).or_default().push((r.written.clone(), new_written));
    }
    if options.dry_run {
        report.rewritten = replace.values().map(|pairs| pairs.len()).sum();
        return Ok(report);
    }

    // Never two files to one path (in any case), which would lose one of them.
    let mut targets: HashMap<String, &str> = HashMap::new();
    for (path, _) in &writes {
        if let Some(other) = targets.insert(path.to_lowercase(), path) {
            return Err(format!("Two files would be written to \"{path}\" (and \"{other}\"); nothing was written."));
        }
    }

    progress("Writing the copy");
    // The copy: every file in the story's folder, media as decided, the story's own files rewritten.
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let media_paths: HashSet<&String> = files.iter().collect();

    copy_others(&story.root, &story.root, out, &media_paths, &replace, &excluded)?;
    for (path, data) in writes {
        let target = out.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        match data {
            Some(bytes) => fs::write(&target, bytes),
            None => fs::copy(media.root.join(&path), &target).map(|_| ()),
        }
        .map_err(|e| format!("{}: {e}", target.display()))?;
    }
    let mut rewritten = 0;
    for (file, pairs) in &replace {
        let Ok(rel) = file.strip_prefix(&story.root) else {
            continue;
        };
        let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
        let (text, n) = rewrite(&text, pairs, story.form == Form::Html && is_html(file));
        rewritten += n;
        fs::write(out.join(rel), text).map_err(|e| e.to_string())?;
    }
    report.rewritten = rewritten;
    Ok(report)
}

fn is_html(file: &Path) -> bool {
    file.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

/// The written path with its file's new name, keeping how it was written (`./`, `%20`).
fn rewrite_written(written: &str, old: &str, new: &str) -> String {
    let old_ext = &old[old.rfind('.').unwrap_or(old.len())..];
    let new_tail = &new[old.rfind('.').unwrap_or(old.len()).min(new.len())..];
    let cut = written.find(['?', '#']).unwrap_or(written.len());
    let (path, query) = written.split_at(cut);
    match path.len().checked_sub(old_ext.len()) {
        Some(at) if path[at..].eq_ignore_ascii_case(old_ext) => {
            format!("{}{new_tail}{query}", &path[..at])
        }
        _ => new.to_string(),
    }
}

/// Replaces each written path where it stands whole (not inside a longer path). In HTML, also as passage text escapes it.
fn rewrite(text: &str, pairs: &[(String, String)], html: bool) -> (String, usize) {
    let mut forms: Vec<(String, String)> = vec![];
    for (old, new) in pairs {
        forms.push((old.clone(), new.clone()));
        if html {
            let escaped = html_escape::encode_double_quoted_attribute(old).into_owned();
            if escaped != *old {
                forms.push((escaped, html_escape::encode_double_quoted_attribute(new).into_owned()));
            }
        }
    }
    forms.sort_by_key(|form| std::cmp::Reverse(form.0.len()));
    forms.dedup();
    let mut out = text.to_string();
    let mut count = 0;
    for (old, new) in forms {
        let mut result = String::with_capacity(out.len());
        let mut last = 0;
        for (at, _) in out.match_indices(old.as_str()) {
            let before = out[..at].chars().next_back();
            let after = out[at + old.len()..].chars().next();
            let path_char = |c: char| c.is_alphanumeric() || matches!(c, '.' | '/' | '_' | '-' | '%' | '\\');
            if before.is_some_and(path_char) || after.is_some_and(path_char) || at < last {
                continue;
            }
            result.push_str(&out[last..at]);
            result.push_str(&new);
            last = at + old.len();
            count += 1;
        }
        result.push_str(&out[last..]);
        out = result;
    }
    (out, count)
}

fn copy_others(
    root: &Path,
    dir: &Path,
    out: &Path,
    media: &HashSet<&String>,
    replace: &HashMap<PathBuf, Vec<(String, String)>>,
    excluded: &dyn Fn(&str) -> bool,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap();
        let rel_str = rel.to_str().unwrap_or_default().replace('\\', "/");
        if path.is_dir() {
            // Not the copy itself, nor folders no copy takes (.git, node_modules).
            if out.starts_with(&path) || path.file_name().and_then(|n| n.to_str()).is_some_and(found::skipped_folder) {
                continue;
            }
            copy_others(root, &path, out, media, replace, excluded)?;
        } else if !media.contains(&rel_str) && !replace.contains_key(&path) && !excluded(&rel_str) {
            let target = out.join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&path, &target).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Encodes one image as WebP: (the bytes, the frame count).
pub fn encode(bytes: &[u8], path: &str, options: &Options) -> Result<(Vec<u8>, usize), String> {
    // By what the file is, not its name: an animated GIF named .jpg still animates in a browser, and must here.
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    if format == image::ImageFormat::WebP {
        return Err(ALREADY_WEBP.into());
    }
    let _ = path;
    if format == image::ImageFormat::Gif {
        let decoder = GifDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let frames = decoder.into_frames().collect_frames().map_err(|e| e.to_string())?;
        if frames.len() > 1 {
            let mut images: Vec<(RgbaImage, i32)> = Vec::with_capacity(frames.len());
            let mut time = 0i32;
            // A browser shows a GIF at the size its header gives (its logical screen), clipping frames that are larger.
            let screen = (u16::from_le_bytes([bytes[6], bytes[7]]) as u32, u16::from_le_bytes([bytes[8], bytes[9]]) as u32);
            for frame in frames {
                let (num, den) = frame.delay().numer_denom_ms();
                // Browsers show a GIF delay under 20ms as 100ms.
                let mut delay = num.checked_div(den).map(|ms| ms as i32).unwrap_or(100);
                if delay < 20 {
                    delay = 100;
                }
                let mut buffer = frame.into_buffer();
                if screen.0 > 0 && screen.1 > 0 && (buffer.width() != screen.0 || buffer.height() != screen.1) {
                    let mut canvas = RgbaImage::new(screen.0, screen.1);
                    image::imageops::replace(&mut canvas, &buffer, 0, 0);
                    buffer = canvas;
                }
                images.push((resized(DynamicImage::ImageRgba8(buffer), options).to_rgba8(), time));
                time += delay;
            }
            let (w, h) = images[0].0.dimensions();
            let config = webp_config(options)?;
            let mut encoder = webp::AnimEncoder::new(w, h, &config);
            encoder.set_loop_count(0);
            for (image, at) in &images {
                encoder.add_frame(webp::AnimFrame::from_rgba(image.as_raw(), w, h, *at));
            }
            let data = encoder.try_encode().map_err(|e| format!("{e:?}"))?;
            return Ok((data.to_vec(), images.len()));
        }
    }
    let image = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
    let image = resized(image, options);
    let (w, h) = image.dimensions();
    let rgba = image.to_rgba8();
    let opaque = rgba.pixels().all(|p| p.0[3] == 255);
    let config = webp_config(options)?;
    let data = if opaque {
        let rgb = image.to_rgb8();
        webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode_advanced(&config)
    } else {
        webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode_advanced(&config)
    }
    .map_err(|e| format!("{e:?}"))?;
    Ok((data.to_vec(), 1))
}

/// libwebp's settings for every file: still or animated, lossy or lossless.
fn webp_config(options: &Options) -> Result<webp::WebPConfig, String> {
    let mut config = webp::WebPConfig::new().map_err(|_| "libwebp's config")?;
    config.lossless = i32::from(options.lossless);
    // Lossy alpha is compressed too; lossless keeps it exact.
    config.alpha_compression = i32::from(!options.lossless);
    config.quality = options.quality;
    config.method = i32::from(options.effort);
    Ok(config)
}

fn resized(image: DynamicImage, options: &Options) -> DynamicImage {
    let (w, h) = image.dimensions();
    let max_w = options.max_width.unwrap_or(u32::MAX);
    let max_h = options.max_height.unwrap_or(u32::MAX);
    if w <= max_w && h <= max_h {
        return image;
    }
    image.resize(max_w.min(w), max_h.min(h), FilterType::Lanczos3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_whole_paths_only_in_both_forms() {
        let text = r#"<img src="a/b c.png"> x/a/b c.png [img[a/b c.png]] src=&quot;a/b c.png&quot;"#;
        let (out, n) = rewrite(text, &[("a/b c.png".into(), "a/b c.webp".into())], true);
        assert_eq!(out, r#"<img src="a/b c.webp"> x/a/b c.png [img[a/b c.webp]] src=&quot;a/b c.webp&quot;"#);
        assert_eq!(n, 3);
    }

    #[test]
    fn keeps_how_a_path_is_written() {
        assert_eq!(rewrite_written("./Day%201/a.GIF?v=1", "Day 1/a.GIF", "Day 1/a.webp"), "./Day%201/a.webp?v=1");
        assert_eq!(rewrite_written("x/a.png", "x/a.png", "x/a.png.webp"), "x/a.png.webp");
        let mut taken: HashSet<String> = ["x/a.webp".to_string()].into();
        assert_eq!(renamed("x/a.png", &mut taken), "x/a.png.webp");
        // Two files with one stem, a.gif and a.jpeg, get two names.
        let mut taken: HashSet<String> = ["y/a.gif".to_string(), "y/a.jpeg".to_string()].into();
        let first = renamed("y/a.gif", &mut taken);
        let second = renamed("y/a.jpeg", &mut taken);
        assert_eq!(first, "y/a.webp");
        assert_eq!(second, "y/a.jpeg.webp");
    }
}
