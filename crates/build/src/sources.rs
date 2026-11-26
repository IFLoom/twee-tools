//! Files beside the Twee: fonts become `@font-face` stylesheets, and (when asked) images, audio, video and subtitles
//! become Twine media passages, as Tweego makes them. Modules (`--module`) go into the page's `<head>`.

use std::path::{Path, PathBuf};

use base64::Engine;
use story::{Code, Passage, Story};

/// A media file's passage tag and MIME type, by extension.
pub fn media_kind(ext: &str) -> Option<(&'static str, &'static str)> {
    Some(match ext {
        "gif" => ("Twine.image", "image/gif"),
        "jpeg" | "jpg" => ("Twine.image", "image/jpeg"),
        "png" => ("Twine.image", "image/png"),
        "svg" => ("Twine.image", "image/svg+xml"),
        "tif" | "tiff" => ("Twine.image", "image/tiff"),
        "webp" => ("Twine.image", "image/webp"),
        "aac" => ("Twine.audio", "audio/aac"),
        "flac" => ("Twine.audio", "audio/flac"),
        "m4a" => ("Twine.audio", "audio/mp4"),
        "mp3" => ("Twine.audio", "audio/mpeg"),
        "oga" | "ogg" | "opus" => ("Twine.audio", "audio/ogg"),
        "wav" | "wave" => ("Twine.audio", "audio/wav"),
        "weba" => ("Twine.audio", "audio/webm"),
        "mp4" => ("Twine.video", "video/mp4"),
        "ogv" => ("Twine.video", "video/ogg"),
        "webm" => ("Twine.video", "video/webm"),
        "vtt" => ("Twine.vtt", "text/vtt"),
        _ => return None,
    })
}

fn font_format(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "ttf" => "truetype",
        "otf" => "opentype",
        "woff" => "woff",
        "woff2" => "woff2",
        _ => return None,
    })
}

/// A font file as a stylesheet: `@font-face` with the font inside, its family the file name up to its first dot.
pub fn font_face(file: &Path) -> Option<String> {
    let ext = extension(file)?;
    let format = font_format(&ext)?;
    let bytes = std::fs::read(file).ok()?;
    let name = file.file_name()?.to_str()?;
    let family = name.split('.').next().unwrap_or(name);
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("@font-face {{\n\tfont-family: \"{family}\";\n\tsrc: url(\"data:font/{ext};base64,{data}\") format(\"{format}\");\n}}"))
}

fn extension(file: &Path) -> Option<String> {
    file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

fn walk(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else { return };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for entry in entries {
        let hidden = entry.file_name().and_then(|n| n.to_str()).is_some_and(story::passed_by);
        if !hidden {
            walk(&entry, out);
        }
    }
}

/// Adds the fonts under the source paths to the story's stylesheets, and, with `embed_media`, its media as passages.
/// Returns how many media passages were made.
pub fn add_assets(story: &mut Story, paths: &[PathBuf], embed_media: bool) -> usize {
    add_assets_except(story, paths, embed_media, &[])
}

/// `add_assets`, leaving out files that match the globs.
pub fn add_assets_except(story: &mut Story, paths: &[PathBuf], embed_media: bool, exclude: &[String]) -> usize {
    let globs = story::glob_set(exclude).ok();
    let mut files = vec![];
    for path in paths {
        walk(path, &mut files);
    }
    files.retain(|f| !story::excluded(f, globs.as_ref().filter(|_| !exclude.is_empty())));
    let mut made = 0;
    for file in files {
        let Some(ext) = extension(&file) else { continue };
        let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        if let Some(css) = font_format(&ext).and_then(|_| font_face(&file)) {
            story.stylesheets.push(Code { name, file: file.clone(), line: 1, text: css });
            continue;
        }
        if !embed_media {
            continue;
        }
        let Some((tag, mime)) = media_kind(&ext) else { continue };
        let Ok(bytes) = std::fs::read(&file) else { continue };
        let passage_name = name.split('.').next().unwrap_or(&name).to_string();
        let text = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
        story.passages.push(Passage {
            name: passage_name,
            tags: vec![tag.into()],
            text,
            file: file.clone(),
            line: 1,
            body_line: 1,
            position: None,
            size: None,
        });
        made += 1;
    }
    made
}

/// Modules for the page's `<head>`: each JavaScript file a `<script>`, each CSS file a `<style>`, each font a `<style>`
/// with its `@font-face`; files in a folder in name order.
pub fn modules(paths: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut files = vec![];
    for path in paths {
        if !path.exists() {
            return Err(format!("{}: no such file or folder", path.display()));
        }
        walk(path, &mut files);
    }
    let mut out = vec![];
    for file in files {
        let Some(ext) = extension(&file) else { continue };
        let stem = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().split('.').next().unwrap_or_default().to_string();
        let slug: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '-' }).collect();
        let read = || std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()));
        match ext.as_str() {
            "js" => {
                out.push(format!("<script id=\"script-module-{slug}\" type=\"text/javascript\">{}</script>", read()?.trim().replace("</script", "<\\/script")))
            }
            "mjs" => out.push(format!("<script id=\"script-module-{slug}\" type=\"module\">{}</script>", read()?.trim().replace("</script", "<\\/script"))),
            "css" => out.push(format!("<style id=\"style-module-{slug}\" type=\"text/css\">{}</style>", read()?.trim())),
            _ => {
                if let Some(css) = font_face(&file) {
                    out.push(format!("<style id=\"style-module-{slug}\" type=\"text/css\">{css}</style>"));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_media_types() {
        assert_eq!(media_kind("jpg"), Some(("Twine.image", "image/jpeg")));
        assert_eq!(media_kind("mp3"), Some(("Twine.audio", "audio/mpeg")));
        assert_eq!(media_kind("vtt"), Some(("Twine.vtt", "text/vtt")));
        assert_eq!(media_kind("txt"), None);
    }
}
