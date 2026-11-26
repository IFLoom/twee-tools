use std::fs;
use std::path::{Path, PathBuf};

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, ImageFormat, Rgba, RgbaImage};
use media::{Action, Options, compress};
use story::Story;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("twee-tools-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A noisy picture, so it's big enough as PNG for WebP to win.
fn picture(w: u32, h: u32, seed: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let v = (x * 7 + y * 13 + seed * 31) % 256;
        Rgba([v as u8, ((x + seed) % 256) as u8, (y % 256) as u8, 255])
    })
}

fn write_story(dir: &Path, passages: &str) {
    let html = format!(
        r#"<html><body><tw-storydata name="T" startnode="1" format="SugarCube" format-version="2.37.3" hidden><style role="stylesheet" id="twine-user-stylesheet" type="text/twine-css">body {{ background: url("media/a.png"); }}</style><script role="script" id="twine-user-script" type="text/twine-javascript">setup.pic = "media/b.jpg";</script>{passages}</tw-storydata></body></html>"#
    );
    fs::write(dir.join("index.html"), html).unwrap();
}

#[test]
fn converts_renames_and_rewrites_without_touching_the_originals() {
    let dir = scratch("story");
    fs::create_dir_all(dir.join("media/day 1")).unwrap();
    picture(320, 240, 1).save_with_format(dir.join("media/a.png"), ImageFormat::Png).unwrap();
    image::DynamicImage::ImageRgba8(picture(320, 240, 2)).to_rgb8().save_with_format(dir.join("media/b.jpg"), ImageFormat::Jpeg).unwrap();
    picture(64, 64, 3).save_with_format(dir.join("media/unused.png"), ImageFormat::Png).unwrap();
    {
        let file = fs::File::create(dir.join("media/day 1/anim.gif")).unwrap();
        let mut encoder = GifEncoder::new(file);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for seed in 0..6 {
            encoder.encode_frame(Frame::from_parts(picture(200, 150, seed), 0, 0, Delay::from_numer_denom_ms(100, 1))).unwrap();
        }
    }
    fs::write(dir.join("readme.txt"), "keep me").unwrap();
    write_story(
        &dir,
        r#"<tw-passagedata pid="1" name="Start" tags="">[img[media/a.png]] &lt;img src="media/day%201/anim.gif"&gt; &lt;img src=&quot;media/a.png&quot;&gt; not media/a.pngx</tw-passagedata>"#,
    );
    let before: Vec<(PathBuf, Vec<u8>)> =
        ["index.html", "media/a.png", "media/b.jpg", "media/day 1/anim.gif"].iter().map(|p| (dir.join(p), fs::read(dir.join(p)).unwrap())).collect();

    let out = scratch("out");
    fs::remove_dir_all(&out).unwrap();
    let story = Story::read(&dir.join("index.html")).unwrap();
    let report = compress(&story, &out, &Options::default()).unwrap();

    // The originals are as they were.
    for (path, bytes) in before {
        assert_eq!(fs::read(&path).unwrap(), bytes, "{} changed", path.display());
    }
    let action = |p: &str| report.files.iter().find(|f| f.path == p).map(|f| f.action.clone()).unwrap();
    assert_eq!(action("media/a.png"), Action::Converted);
    assert_eq!(action("media/b.jpg"), Action::Converted);
    assert_eq!(action("media/day 1/anim.gif"), Action::Converted);
    assert_eq!(action("media/unused.png"), Action::Kept("nothing uses it".into()));
    let gif = report.files.iter().find(|f| f.path.ends_with("anim.gif")).unwrap();
    assert_eq!(gif.frames, 6);
    assert!(report.after < report.before);

    // The copy has the new files, the untouched ones, and a story that points at the new names.
    assert!(out.join("media/a.webp").is_file() && !out.join("media/a.png").exists());
    assert!(out.join("media/day 1/anim.webp").is_file());
    assert!(out.join("media/unused.png").is_file() && out.join("readme.txt").is_file());
    let animated = webp::AnimDecoder::new(&fs::read(out.join("media/day 1/anim.webp")).unwrap()).decode().unwrap();
    assert_eq!(animated.len(), 6);
    let html = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(html.contains("[img[media/a.webp]]"), "{html}");
    assert!(html.contains("media/day%201/anim.webp"), "{html}");
    assert!(html.contains("src=&quot;media/a.webp&quot;"), "{html}");
    assert!(html.contains("url(\"media/a.webp\")") && html.contains("setup.pic = \"media/b.webp\""), "{html}");
    assert!(html.contains("not media/a.pngx"), "a longer word that starts with the path is left alone");
    assert!(!html.contains("media/a.png\"") && !html.contains("media/b.jpg"));

    // The copy checks clean of missing media.
    let copy = Story::read(&out.join("index.html")).unwrap();
    let findings = check::check(&copy, &check::config::Config::default());
    assert!(findings.findings.iter().all(|f| f.code != "missing-media"), "{:?}", findings.findings);
    fs::remove_dir_all(&dir).unwrap();
    fs::remove_dir_all(&out).unwrap();
}

#[test]
fn refuses_to_write_into_the_story_folder_and_leaves_built_paths_alone() {
    let dir = scratch("dynamic");
    fs::create_dir_all(dir.join("rooms")).unwrap();
    picture(320, 240, 4).save_with_format(dir.join("rooms/hall.png"), ImageFormat::Png).unwrap();
    write_story(&dir, r#"<tw-passagedata pid="1" name="Start" tags="">&lt;img @src="'rooms/' + $room + '.png'"&gt;</tw-passagedata>"#);
    let story = Story::read(&dir.join("index.html")).unwrap();
    assert!(compress(&story, &dir.join("out"), &Options::default()).is_err());

    let dry = compress(&story, Path::new(""), &Options { dry_run: true, ..Options::default() }).unwrap();
    assert_eq!(dry.files.iter().find(|f| f.path == "rooms/hall.png").unwrap().action, Action::Kept("its path is built as the story runs".into()));
    assert!(dry.out.is_none());
    let kept = compress(&story, Path::new(""), &Options { dry_run: true, keep_names: true, ..Options::default() }).unwrap();
    assert_eq!(kept.files.iter().find(|f| f.path == "rooms/hall.png").unwrap().action, Action::ConvertedInPlace);
    fs::remove_dir_all(&dir).unwrap();
}

/// An animated GIF named .jpg is converted as an animation, and a WebP named .gif is left alone.
#[test]
fn goes_by_what_a_file_is_not_its_name() {
    let mut gif = vec![];
    {
        let mut encoder = GifEncoder::new(&mut gif);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for seed in 0..4 {
            encoder.encode_frame(Frame::from_parts(picture(120, 90, seed), 0, 0, Delay::from_numer_denom_ms(80, 1))).unwrap();
        }
    }
    let (data, frames) = media::encode(&gif, "x/anim.jpg", &Options::default()).unwrap();
    assert_eq!(frames, 4);
    assert_eq!(webp::AnimDecoder::new(&data).decode().unwrap().len(), 4);
    let error = media::encode(&data, "x/really-webp.gif", &Options::default()).unwrap_err();
    assert_eq!(error, media::ALREADY_WEBP);
}

/// A GIF whose frames are larger than the size its header gives is shown at the header's size, as browsers do.
#[test]
fn keeps_a_gif_at_its_declared_size() {
    let mut gif = vec![];
    {
        let mut encoder = GifEncoder::new(&mut gif);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for seed in 0..3 {
            encoder.encode_frame(Frame::from_parts(picture(200, 100, seed), 0, 0, Delay::from_numer_denom_ms(80, 1))).unwrap();
        }
    }
    // Shrink the logical screen in the header to 120 x 60.
    gif[6..8].copy_from_slice(&120u16.to_le_bytes());
    gif[8..10].copy_from_slice(&60u16.to_le_bytes());
    let (data, frames) = media::encode(&gif, "big.gif", &Options::default()).unwrap();
    assert_eq!(frames, 3);
    let decoded = webp::AnimDecoder::new(&data).decode().unwrap();
    let first = decoded.get_frame(0).unwrap();
    assert_eq!((first.width(), first.height()), (120, 60));
}

#[test]
fn the_copy_leaves_out_tool_folders_and_keeps_hidden_files() {
    let dir = scratch("folders");
    fs::create_dir_all(dir.join("media")).unwrap();
    picture(64, 64, 1).save_with_format(dir.join("media/a.png"), ImageFormat::Png).unwrap();
    picture(64, 64, 2).save_with_format(dir.join("media/b.jpg"), ImageFormat::Png).unwrap();
    for (file, text) in
        [(".git/config", "x"), ("node_modules/pkg/index.js", "y"), ("target/debug/z", "z"), (".htaccess", "Options -Indexes"), (".nojekyll", "")]
    {
        fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
        fs::write(dir.join(file), text).unwrap();
    }
    write_story(&dir, r#"<tw-passagedata pid="1" name="Start" tags="">Hi.</tw-passagedata>"#);
    let out = scratch("folders-out");
    fs::remove_dir_all(&out).unwrap();
    compress(&Story::read(&dir.join("index.html")).unwrap(), &out, &Options::default()).unwrap();
    for gone in [".git", "node_modules", "target"] {
        assert!(!out.join(gone).exists(), "{gone} was copied");
    }
    assert_eq!(fs::read_to_string(out.join(".htaccess")).unwrap(), "Options -Indexes");
    assert!(out.join(".nojekyll").exists());
    fs::remove_dir_all(&dir).unwrap();
    fs::remove_dir_all(&out).unwrap();
}

#[test]
fn refuses_a_story_whose_media_are_elsewhere() {
    let dir = scratch("elsewhere");
    write_story(&dir, r#"<tw-passagedata pid="1" name="Start" tags="">Hi.</tw-passagedata>"#);
    let mut story = Story::read(&dir.join("index.html")).unwrap();
    story.media_root = dir.join("dist");
    let error = compress(&story, &dir.join("out"), &Options::default()).unwrap_err();
    assert!(error.contains("compress the page that's beside its media"), "{error}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn exclude_leaves_unused_files_out_and_refuses_used_ones() {
    let dir = scratch("exclude");
    fs::create_dir_all(dir.join("media")).unwrap();
    picture(64, 64, 1).save_with_format(dir.join("media/a.png"), ImageFormat::Png).unwrap();
    picture(64, 64, 2).save_with_format(dir.join("media/b.jpg"), ImageFormat::Png).unwrap();
    picture(64, 64, 3).save_with_format(dir.join("media/draft.png"), ImageFormat::Png).unwrap();
    fs::write(dir.join("cover.psd"), "layers").unwrap();
    write_story(&dir, r#"<tw-passagedata pid="1" name="Start" tags="">Hi.</tw-passagedata>"#);
    let story = Story::read(&dir.join("index.html")).unwrap();
    let out = scratch("exclude-out");
    fs::remove_dir_all(&out).unwrap();
    let options = Options { exclude: vec!["*.psd".into(), "media/draft.png".into()], ..Options::default() };
    let report = compress(&story, &out, &options).unwrap();
    assert!(!out.join("cover.psd").exists() && !out.join("media/draft.png").exists() && !out.join("media/draft.webp").exists());
    assert!(out.join("media/a.webp").exists(), "{:?}", report.files);
    assert!(!report.unused.iter().any(|f| f == "media/draft.png"), "an excluded file isn't reported as unused");
    fs::remove_dir_all(&out).unwrap();

    // media/a.png is used by the stylesheet: leaving it out would break the copy.
    let error = compress(&story, &out, &Options { exclude: vec!["media/a.png".into()], ..Options::default() }).unwrap_err();
    assert!(error.contains("media/a.png") && error.contains("uses it"), "{error}");
    assert!(!out.exists(), "nothing was written");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn lossless_keeps_every_pixel_and_bad_settings_are_refused() {
    let dir = scratch("lossless");
    fs::create_dir_all(dir.join("media")).unwrap();
    let original = picture(96, 64, 4);
    original.save_with_format(dir.join("media/a.png"), ImageFormat::Png).unwrap();
    picture(64, 64, 5).save_with_format(dir.join("media/b.jpg"), ImageFormat::Png).unwrap();
    write_story(&dir, r#"<tw-passagedata pid="1" name="Start" tags="">Hi.</tw-passagedata>"#);
    let story = Story::read(&dir.join("index.html")).unwrap();
    let out = scratch("lossless-out");
    fs::remove_dir_all(&out).unwrap();
    compress(&story, &out, &Options { lossless: true, effort: 6, ..Options::default() }).unwrap();
    let back = image::open(out.join("media/a.webp")).unwrap().to_rgba8();
    assert_eq!(back.as_raw(), original.as_raw(), "lossless keeps every pixel");
    fs::remove_dir_all(&out).unwrap();
    for (options, words) in [(Options { effort: 7, ..Options::default() }, "effort is 7"), (Options { quality: 120.0, ..Options::default() }, "quality is 120")]
    {
        let error = compress(&story, &out, &options).unwrap_err();
        assert!(error.contains(words), "{error}");
    }
    fs::remove_dir_all(&dir).unwrap();
}
