//! `check` reads published pages; `build` (and `build --check-only`) reads Twee. On one project they must give the same
//! answer: build stops exactly when `build --check-only` fails, and `check` on the page agrees with both.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_twee-tools");

/// A project in a fresh temp folder: twee.toml, a story, a draft the project excludes (with a broken link), and an image
/// beside where the page is built, where the browser will look for it.
fn project(name: &str, story_extra: &str, toml_extra: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("twee-tools-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files: &[(&str, String)] = &[
        ("twee.toml", format!("sources = [\"src\"]\noutput = \"dist/index.html\"\nexclude = [\"src/drafts/**\"]\n{toml_extra}")),
        (
            "src/story.twee",
            format!(
                ":: StoryTitle\nDemo\n\n:: StoryData\n{{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"SugarCube\", \
                 \"format-version\": \"2.37.3\", \"start\": \"Start\"}}\n\n:: Start\n<img src=\"media/sea.png\">\n[[The End]]\n{story_extra}\n\n\
                 :: The End\nThanks for playing.\n"
            ),
        ),
        ("src/drafts/old.twee", ":: Old idea\nThis went to [[Nowhere]].\n".into()),
        ("dist/media/sea.png", "not really a PNG".into()),
    ];
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN).args(args).arg("--color").arg("never").current_dir(dir).output().unwrap()
}

/// A command with no colour option of its own (`init`, `formats`).
fn plain(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN).args(args).current_dir(dir).output().unwrap()
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

#[test]
fn check_refuses_twee_and_points_to_build() {
    let dir = project("refuse", "", "");
    let out = run(&dir, &["check", "src"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("build --check-only"), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_only_reads_the_project_as_the_build_does_and_writes_nothing() {
    // The excluded draft's broken link isn't read, and the image is found beside the page.
    let dir = project("clean", "", "");
    let out = run(&dir, &["build", "--check-only"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(!dir.join("dist/index.html").exists());
    let built = run(&dir, &["build"]);
    assert_eq!(built.status.code(), Some(0), "{}", text(&built));
    let page = run(&dir, &["check", "dist/index.html"]);
    assert_eq!(page.status.code(), Some(0), "{}", text(&page));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_sources_and_the_page_agree_on_a_broken_link() {
    let dir = project("broken", "[[Nowhere]]", "");
    let checked = run(&dir, &["build", "--check-only"]);
    assert_eq!(checked.status.code(), Some(1), "{}", text(&checked));
    assert!(text(&checked).contains("Nowhere") && text(&checked).contains("story.twee"), "{}", text(&checked));
    let built = run(&dir, &["build"]);
    assert_eq!(built.status.code(), Some(1), "{}", text(&built));
    assert!(!dir.join("dist/index.html").exists(), "a stopped build writes nothing");
    assert_eq!(run(&dir, &["build", "--no-check"]).status.code(), Some(0));
    let page = run(&dir, &["check", "dist/index.html"]);
    assert_eq!(page.status.code(), Some(1), "{}", text(&page));
    assert!(text(&page).contains("Nowhere"), "{}", text(&page));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn strict_fails_on_a_warning_both_ways() {
    // A passage nothing leads to is a warning.
    let dir = project("strict", "\n:: Attic\nDusty.", "strict = true\n");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(1));
    assert_eq!(run(&dir, &["build"]).status.code(), Some(1));
    std::fs::remove_dir_all(&dir).unwrap();
    let dir = project("lenient", "\n:: Attic\nDusty.", "");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn checker_settings_come_from_twee_toml_or_the_command_line() {
    let macros = "sugarcube-2:\n  macros:\n    sparkle:\n      container: false\n";
    let dir = project("config", "<<sparkle>>", "\n[check]\nconfig = [\"extra.twee-config.yml\"]\n");
    std::fs::write(dir.join("extra.twee-config.yml"), macros).unwrap();
    let out = run(&dir, &["build", "--check-only"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();

    let dir = project("config-flag", "<<sparkle>>", "");
    std::fs::write(dir.join("extra.twee-config.yml"), macros).unwrap();
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(1), "unknown without the settings");
    let out = run(&dir, &["build", "--check-only", "--config", "extra.twee-config.yml"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_only_prints_json() {
    let dir = project("json", "", "");
    let out = run(&dir, &["build", "--check-only", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a JSON report");
    assert!(report["findings"].is_array() && report["stats"]["passages"].as_u64() == Some(2), "{report}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_reads_a_published_page() {
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/small/story.html");
    let out = Command::new(BIN).args(["check", "--json"]).arg(&page).output().unwrap();
    assert!(matches!(out.status.code(), Some(0 | 1)), "{}", text(&out));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("a JSON report");
    assert!(report["stats"]["passages"].as_u64().unwrap() > 0);
}

#[test]
fn exclude_on_the_command_line_adds_to_twee_toml() {
    // The project excludes src/drafts; --exclude also leaves out *.bak files. Each holds a broken link.
    let dir = project("exclude", "", "");
    std::fs::write(dir.join("src/old.bak.twee"), ":: Older idea\nThis went to [[Elsewhere]].\n").unwrap();
    let without = run(&dir, &["build", "--check-only"]);
    assert_eq!(without.status.code(), Some(1), "{}", text(&without));
    assert!(text(&without).contains("Elsewhere") && !text(&without).contains("Nowhere"), "{}", text(&without));
    let with = run(&dir, &["build", "--check-only", "--exclude", "**/*.bak.twee"]);
    assert_eq!(with.status.code(), Some(0), "{}", text(&with));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_release_copy_leaves_out_what_compress_excludes() {
    let dir = project("release-exclude", "", "\n[compress]\nenabled = true\nexclude = [\"**/*.psd\", \"notes/**\"]\n");
    std::fs::remove_file(dir.join("dist/media/sea.png")).unwrap();
    std::fs::write(dir.join("src/story.twee"), std::fs::read_to_string(dir.join("src/story.twee")).unwrap().replace("<img src=\"media/sea.png\">", ""))
        .unwrap();
    for (file, text) in [("dist/art/cover.psd", "layers"), ("dist/notes/todo.txt", "fix"), ("dist/credits.txt", "thanks")] {
        std::fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
        std::fs::write(dir.join(file), text).unwrap();
    }
    let out = run(&dir, &["build"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let release = dir.join("release");
    assert!(release.join("index.html").exists() && release.join("credits.txt").exists(), "{}", text(&out));
    assert!(!release.join("art/cover.psd").exists() && !release.join("notes").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A real 4 by 4 PNG, for the release copy to encode.
const PNG: [u8; 98] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 2, 0, 0, 0, 38, 147, 9, 41, 0, 0, 0, 41, 73, 68, 65, 84, 120, 156,
    13, 199, 49, 1, 0, 0, 12, 194, 48, 132, 85, 24, 103, 69, 33, 112, 203, 151, 36, 18, 27, 23, 19, 4, 139, 227, 83, 169, 173, 235, 103, 50, 59, 55, 15, 167,
    99, 19, 65, 54, 29, 73, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// The release copy of a project whose page shows a PNG, built with these extra arguments; its WebP, or the build's output.
fn release_image(name: &str, toml_extra: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    let dir = project(name, "", toml_extra);
    std::fs::write(dir.join("dist/media/sea.png"), PNG).unwrap();
    let mut all = vec!["build", "--compress-to", "release"];
    all.extend(args);
    let out = run(&dir, &all);
    let result = if out.status.success() { Ok(std::fs::read(dir.join("release/media/sea.webp")).unwrap()) } else { Err(text(&out)) };
    std::fs::remove_dir_all(&dir).unwrap();
    result
}

#[test]
fn twee_toml_sets_the_release_copy_encoding_and_flags_win() {
    let lossy = release_image("enc-lossy", "", &[]).unwrap();
    let lossless = release_image("enc-lossless", "\n[compress]\nlossless = true\n", &[]).unwrap();
    // A lossless WebP is a VP8L stream; a lossy one VP8.
    assert_eq!(&lossy[12..16], b"VP8 ");
    assert_eq!(&lossless[12..16], b"VP8L", "twee.toml's lossless applies with --compress-to too");
    assert_eq!(release_image("enc-flag", "", &["--lossless"]).unwrap()[12..16], *b"VP8L");
    let error = release_image("enc-bad", "\n[compress]\neffort = 9\n", &[]).unwrap_err();
    assert!(error.contains("effort is 9"), "{error}");
    assert!(release_image("enc-flag-wins", "\n[compress]\neffort = 9\n", &["--effort", "6"]).is_ok(), "the flag wins over twee.toml");
}

#[test]
fn twee_toml_is_found_from_a_folder_below_and_with_dash_c() {
    let dir = project("found-up", "", "");
    let below = dir.join("src/drafts");
    assert_eq!(run(&below, &["build", "--check-only"]).status.code(), Some(0), "found from two folders below");
    let elsewhere = std::env::temp_dir();
    let out = Command::new(BIN).args(["-C"]).arg(&dir).args(["build", "--check-only", "--color", "never"]).current_dir(&elsewhere).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn requires_a_new_enough_twee_tools() {
    let dir = project("requires", "", "requires = \"99.0\"\n");
    let out = run(&dir, &["build"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("needs twee-tools 99.0 or later"), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();
    let dir = project("requires-ok", "", "requires = \"0.1\"\n");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn init_pins_the_story_format() {
    let dir = std::env::temp_dir().join(format!("twee-tools-cli-init-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let made = plain(&dir, &["init", "story", "--format", "harlowe"]);
    assert!(made.status.success(), "{}", text(&made));
    let toml = std::fs::read_to_string(dir.join("story/twee.toml")).unwrap();
    assert!(toml.lines().any(|l| l.starts_with("format = \"Harlowe@")), "{toml}");
    assert_eq!(run(&dir.join("story"), &["build", "--check-only"]).status.code(), Some(0));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn ignoring_and_changing_severities() {
    // A passage nothing leads to is a warning; strict makes it fail.
    let dir = project("ignore", "\n:: Attic\nDusty.", "strict = true\n\n[check]\nignore = [\"unreachable\"]\n");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(0), "ignored in twee.toml");
    std::fs::remove_dir_all(&dir).unwrap();
    let dir = project("ignore-flag", "\n:: Attic\nDusty.", "strict = true\n");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(1));
    assert_eq!(run(&dir, &["build", "--check-only", "--ignore", "unreachable"]).status.code(), Some(0), "ignored on the command line");
    std::fs::remove_dir_all(&dir).unwrap();
    let dir = project("ignore-tag", "\n:: Attic [twee-tools-ignore-unreachable]\nDusty.", "strict = true\n");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(0), "ignored by the passage's tag");
    std::fs::remove_dir_all(&dir).unwrap();
    let dir = project("severity", "\n:: Attic\nDusty.", "\n[check]\nseverity = { unreachable = \"error\" }\n");
    assert_eq!(run(&dir, &["build", "--check-only"]).status.code(), Some(1), "raised to an error, it fails without strict");
    assert_eq!(run(&dir, &["build"]).status.code(), Some(1));
    assert_eq!(run(&dir, &["build", "--no-check"]).status.code(), Some(0));
    // check on the page reads the project's [check] too.
    assert_eq!(run(&dir, &["check", "dist/index.html"]).status.code(), Some(1), "check reads the project's severities");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn assets_are_checked_where_they_will_be_and_copied_after_a_good_build() {
    let dir = project("assets", "", "assets = [\"src/media\"]\n");
    // The image now lives with the sources, not beside the page.
    std::fs::remove_dir_all(dir.join("dist")).unwrap();
    std::fs::create_dir_all(dir.join("src/media")).unwrap();
    std::fs::write(dir.join("src/media/sea.png"), PNG).unwrap();
    let checked = run(&dir, &["build", "--check-only"]);
    assert_eq!(checked.status.code(), Some(0), "the image is found where it will be: {}", text(&checked));
    assert!(!dir.join("dist").exists(), "--check-only writes nothing, assets included");
    let built = run(&dir, &["build"]);
    assert_eq!(built.status.code(), Some(0), "{}", text(&built));
    assert_eq!(std::fs::read(dir.join("dist/media/sea.png")).unwrap(), PNG);
    assert!(text(&built).contains("1 asset files copied"), "{}", text(&built));
    assert!(!text(&run(&dir, &["build"])).contains("asset files copied"), "an unchanged asset isn't copied again");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_projects_own_formats_win() {
    let dir = project("formats", "", "format-paths = [\"formats\"]\n");
    std::fs::create_dir_all(dir.join("formats/sugarcube-2")).unwrap();
    std::fs::write(
        dir.join("formats/sugarcube-2/format.js"),
        "window.storyFormat({\"name\":\"SugarCube\",\"version\":\"2.37.99\",\"source\":\"<html><head><title>{{STORY_NAME}}</title></head><body>{{STORY_DATA}}</body></html>\"});",
    )
    .unwrap();
    let listed = plain(&dir, &["formats"]);
    assert!(text(&listed).contains("2.37.99"), "{}", text(&listed));
    let built = run(&dir, &["build"]);
    assert!(text(&built).contains("SugarCube 2.37.99"), "{}", text(&built));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A project whose image lives in a folder of things to copy as they are, not yet beside the page.
fn with_public(name: &str, folder: &str, toml_extra: &str) -> PathBuf {
    let dir = project(name, "", toml_extra);
    std::fs::remove_dir_all(dir.join("dist")).unwrap();
    std::fs::create_dir_all(dir.join(folder).join("media")).unwrap();
    std::fs::write(dir.join(folder).join("media/sea.png"), PNG).unwrap();
    std::fs::write(dir.join(folder).join("robots.txt"), "User-agent: *\n").unwrap();
    dir
}

#[test]
fn public_is_copied_as_it_is_laid_out_by_default() {
    let dir = with_public("public", "public", "");
    let checked = run(&dir, &["build", "--check-only"]);
    assert_eq!(checked.status.code(), Some(0), "media in public/ are found where they'll be: {}", text(&checked));
    assert!(!dir.join("dist").exists(), "--check-only copies nothing");
    let built = run(&dir, &["build"]);
    assert_eq!(built.status.code(), Some(0), "{}", text(&built));
    assert_eq!(std::fs::read(dir.join("dist/media/sea.png")).unwrap(), PNG, "public/media/sea.png is dist/media/sea.png");
    assert!(dir.join("dist/robots.txt").exists(), "files at the top of public/ go beside the page");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn public_can_be_another_folder_or_off() {
    let dir = with_public("public-named", "static", "public = \"static\"\n");
    assert_eq!(run(&dir, &["build"]).status.code(), Some(0));
    assert!(dir.join("dist/media/sea.png").exists());
    std::fs::remove_dir_all(&dir).unwrap();

    let dir = with_public("public-off", "public", "public = false\n");
    let out = run(&dir, &["build", "--check-only"]);
    assert_eq!(out.status.code(), Some(1), "off, the image isn't there: {}", text(&out));
    assert!(text(&out).contains("media/sea.png"), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();

    let dir = project("public-missing", "", "public = \"nowhere\"\n");
    let out = run(&dir, &["build"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("no such folder"), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn public_never_replaces_the_built_page() {
    let dir = with_public("public-index", "public", "");
    std::fs::write(dir.join("public/index.html"), "not the story").unwrap();
    let out = run(&dir, &["build"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(std::fs::read_to_string(dir.join("dist/index.html")).unwrap().contains("tw-storydata"), "the built page stays");
    assert!(text(&out).contains("has the built page's name"), "{}", text(&out));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn init_makes_a_public_folder() {
    let dir = std::env::temp_dir().join(format!("twee-tools-cli-init-public-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(plain(&dir, &["init", "story"]).status.success());
    assert!(dir.join("story/public").is_dir());
    assert_eq!(run(&dir.join("story"), &["build"]).status.code(), Some(0));
    assert!(!dir.join("story/dist/.gitkeep").exists(), "hidden files aren't copied");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn passages_keep_their_ends_with_no_trim() {
    let dir = project("no-trim", "", "");
    std::fs::write(dir.join("src/poem.twee"), ":: Poem\n\n    Indented first line.\n").unwrap();
    assert!(run(&dir, &["build"]).status.success());
    let trimmed = std::fs::read_to_string(dir.join("dist/index.html")).unwrap();
    assert!(trimmed.contains(">Indented first line.</tw-passagedata>"), "trimmed by default");
    assert!(run(&dir, &["build", "--no-trim"]).status.success());
    let kept = std::fs::read_to_string(dir.join("dist/index.html")).unwrap();
    assert!(
        kept.contains(">\n    Indented first line.\n</tw-passagedata>") || kept.contains(">\n    Indented first line.</tw-passagedata>"),
        "kept with --no-trim"
    );
    std::fs::write(dir.join("twee.toml"), std::fs::read_to_string(dir.join("twee.toml")).unwrap() + "trim = false\n").unwrap();
    assert!(run(&dir, &["build"]).status.success());
    assert!(!std::fs::read_to_string(dir.join("dist/index.html")).unwrap().contains(">Indented first line."), "kept with trim = false");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_report_counts_words() {
    let dir = project("words", "", "");
    let out = run(&dir, &["build", "--check-only", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(report["stats"]["words"].as_u64().unwrap() > 0, "{report}");
    assert!(text(&run(&dir, &["build", "--check-only"])).contains(" words"), "the report's header says how many");
    std::fs::remove_dir_all(&dir).unwrap();
}
