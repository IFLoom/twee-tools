use std::path::{Path, PathBuf};

use story::{Form, Story};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The same story, from Twee and from published HTML, reads as the same model.
#[test]
fn twee_and_html_read_the_same() {
    let twee = Story::read(&fixtures().join("small/story.twee")).unwrap();
    let html = Story::read(&fixtures().join("small/story.html")).unwrap();
    assert_eq!((twee.form, html.form), (Form::Twee, Form::Html));
    assert_eq!(twee.title.as_deref(), Some("The Bakery"));
    assert_eq!(twee.title, html.title);
    assert_eq!(twee.format, html.format);
    assert_eq!(twee.format_version, html.format_version);
    assert_eq!(twee.start.as_deref(), Some("Street"));
    assert_eq!(twee.start, html.start);
    let shape = |s: &Story| s.passages.iter().map(|p| (p.name.clone(), p.tags.clone(), p.text.clone())).collect::<Vec<_>>();
    assert_eq!(shape(&twee), shape(&html));
    assert_eq!(twee.passages.len(), 4);
    assert!(html.passage("Home").unwrap().text.contains("bread & smile"));
    let code = |s: &Story| {
        (s.scripts.iter().map(|c| c.text.trim().to_string()).collect::<Vec<_>>(), s.stylesheets.iter().map(|c| c.text.trim().to_string()).collect::<Vec<_>>())
    };
    assert_eq!(code(&twee), code(&html));
    // Lines point at each passage's header in either form.
    assert_eq!(twee.passage("Bakery").unwrap().line, 19);
    assert!(html.passage("Bakery").unwrap().line > 1);
}

fn files(dir: &Path, ext: &[&str], out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, ext, out)
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| ext.contains(&e)) {
            out.push(path)
        }
    }
}

/// Every Twee file in Extwee's and twee-ts's tests reads without a panic; only StoryData that isn't JSON fails.
#[test]
fn reads_every_external_twee_fixture() {
    let root = fixtures().join("external");
    if !root.exists() {
        eprintln!("fixtures/external isn't cloned; skipped");
        return;
    }
    let mut twee = vec![];
    files(&root.join("extwee/test"), &["twee", "tw"], &mut twee);
    files(&root.join("twee-ts/test"), &["twee", "tw"], &mut twee);
    assert!(twee.len() > 20, "found {}", twee.len());
    let mut failed = vec![];
    for file in &twee {
        if let Err(error) = Story::read(file) {
            failed.push(error.to_string());
        }
    }
    assert!(failed.iter().all(|e| e.contains("StoryData isn't JSON")), "{failed:#?}");

    let tags = Story::read(&root.join("extwee/test/Twee/TweeParser/multipletags.twee")).unwrap();
    assert!(tags.passages.iter().any(|p| p.tags.len() >= 2), "{:?}", tags.passages);
}

/// Every Twine 2 HTML fixture in Extwee's tests with a <tw-storydata> reads, with the start passage it names.
#[test]
fn reads_external_twine2_html() {
    let dir = fixtures().join("external/extwee/test/Twine2HTML/Twine2HTMLParser");
    if !dir.exists() {
        return;
    }
    let example = Story::read(&dir.join("Example1.html")).unwrap();
    assert_eq!(example.title.as_deref(), Some("Example1"));
    assert!(example.passages.len() >= 2);
    assert!(example.start.is_some());
    let mut html = vec![];
    files(&dir, &["html"], &mut html);
    let read = html.iter().filter(|f| Story::read(f).is_ok()).count();
    assert!(read * 2 > html.len(), "{read} of {} read", html.len());
}

/// Several paths read as one story, each file once, as Tweego reads them.
#[test]
fn reads_several_paths_as_one_story() {
    let dir = std::env::temp_dir().join(format!("twee-tools-many-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src/rooms")).unwrap();
    std::fs::write(dir.join("src/main.twee"), ":: StoryData\n{\"format\": \"SugarCube\", \"start\": \"Hall\"}\n\n:: Hall\n[[Den]]\r\n").unwrap();
    std::fs::write(dir.join("src/rooms/den.tw"), "::Den\r\nCosy.\r\n").unwrap();
    std::fs::write(dir.join("src/style.css"), "body {}").unwrap();
    std::fs::write(dir.join("extra.twee"), ":: Attic\nDusty.\n").unwrap();
    let story = Story::read_all(&[dir.join("src"), dir.join("extra.twee"), dir.join("src/rooms/den.tw")]).unwrap();
    let mut names: Vec<_> = story.passages.iter().map(|p| p.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Attic", "Den", "Hall"]);
    assert_eq!(story.files.len(), 3);
    assert_eq!(story.stylesheets.len(), 1);
    assert_eq!(story.passage("Den").unwrap().text, "Cosy.");
    assert!(Story::read_all(&[dir.join("src"), fixtures().join("small/story.html")]).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A folder with no Twee files is a published story's folder: its one story is read; none or several is an error.
#[test]
fn reads_the_published_story_in_a_folder_without_twee() {
    let small = Story::read(&fixtures().join("small")).unwrap();
    assert_eq!((small.form, small.passages.len()), (Form::Twee, 4), "the small fixture has Twee, which wins");
    let dir = std::env::temp_dir().join(format!("twee-tools-published-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("media")).unwrap();
    assert!(Story::read(&dir).unwrap_err().to_string().contains("no Twee files"));
    std::fs::copy(fixtures().join("small/story.html"), dir.join("game.html")).unwrap();
    std::fs::write(dir.join("notes.html"), "<p>not a story</p>").unwrap();
    let story = Story::read(&dir).unwrap();
    assert_eq!((story.form, story.passages.len()), (Form::Html, 4));
    std::fs::copy(fixtures().join("small/story.html"), dir.join("other.html")).unwrap();
    assert!(Story::read(&dir).unwrap_err().to_string().contains("2 published stories"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_name_means_the_last_passage_of_that_name_in_the_first_ones_place() {
    let text = ":: Start\n[[Den]]\n\n:: Den\nFirst den.\n\n:: Hall\nA hall.\n\n:: Den\nSecond den.\n\n:: Den\nThird den.\n";
    let story = story::Story::from_twee(text, std::path::Path::new("story.twee")).unwrap();
    let names: Vec<&str> = story.passages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Start", "Den", "Hall"]);
    assert_eq!(story.passage("Den").unwrap().text, "Third den.");
    let lines: Vec<(usize, usize)> = story.duplicates.iter().map(|d| (d.line, d.first_line)).collect();
    assert_eq!(lines, [(10, 4), (13, 4)], "each later declaration, with the first one's line");
}

#[test]
fn a_published_page_with_two_passages_of_a_name_keeps_the_last() {
    let html = r#"<tw-storydata name="T" startnode="1" format="SugarCube" format-version="2.37.3"><tw-passagedata pid="1" name="Start">One.</tw-passagedata><tw-passagedata pid="2" name="Start">Two.</tw-passagedata></tw-storydata>"#;
    let story = story::read_html(html, std::path::Path::new("story.html")).unwrap();
    assert_eq!(story.passages.len(), 1);
    assert_eq!(story.passages[0].text, "Two.");
    assert_eq!(story.duplicates.len(), 1);
}

#[test]
fn versions_and_skipped_folders() {
    assert_eq!(story::parse_version("2.37.3"), Some((2, 37, 3)));
    assert_eq!(story::parse_version("3.3.9-beta+1"), Some((3, 3, 9)));
    assert_eq!(story::parse_version("2.37"), None);
    assert!(story::skipped_folder(".git") && story::skipped_folder("node_modules") && story::skipped_folder("target"));
    assert!(!story::skipped_folder("media") && story::passed_by(".DS_Store") && !story::passed_by("story.twee"));
}
