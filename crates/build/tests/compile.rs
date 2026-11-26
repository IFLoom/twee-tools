//! Compiling: Twee in, a page out that reads back as the same story.

use std::path::PathBuf;

use build::formats::{self, Format};
use build::output::{Options, compile};
use story::Story;

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("twee-tools-build-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (file, text) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn sugarcube() -> Format {
    let (all, _) = formats::all(&[]);
    all.into_iter().find(|f| f.id == "sugarcube-2").unwrap()
}

const STORY: &str = r#":: StoryTitle
Tom & Jo's <Story>

:: StoryData
{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "format": "SugarCube", "format-version": "2.37.3", "start": "Hall", "tag-colors": {"cold": "blue"}}

:: Hall [cold dark] {"position":"600,400","size":"200,100"}
"Hello," says <b>Jo</b> & waves. [[Den]]

:: Den
A den.

:: Den
The second den wins.

:: Notes [Twine.private]
Not published.

:: UI [script]
var a = "</script>";
"#;

#[test]
fn compiles_and_reads_back_as_the_same_story() {
    let dir = scratch("roundtrip", &[("src/story.twee", STORY), ("src/extra.js", "var b = 2;"), ("src/look.css", "body { color: red; }")]);
    let story = Story::read(&dir.join("src")).unwrap();
    let page = compile(&story, &sugarcube(), &Options::default()).unwrap();
    assert_eq!(page.passages, 2);
    assert!(page.notes.iter().any(|n| n.contains("\"Den\" is declared more than once")));
    assert!(page.html.contains("<title>Tom &amp; Jo&#39;s &lt;Story&gt;</title>"), "the title is escaped where the format puts it");
    assert!(page.html.contains(r#"<tw-tag name="cold" color="blue"></tw-tag>"#));
    assert!(page.html.contains(r#"var a = "<\/script>";"#), "a closing tag in user script can't end the element");
    assert!(!page.html.contains("Not published"));
    assert!(!page.html.contains("{{STORY_DATA}}") && !page.html.contains("{{STORY_NAME}}"));

    std::fs::write(dir.join("out.html"), &page.html).unwrap();
    let back = Story::read(&dir.join("out.html")).unwrap();
    assert_eq!(back.title.as_deref(), Some("Tom & Jo's <Story>"));
    assert_eq!(back.start.as_deref(), Some("Hall"));
    assert_eq!(back.ifid.as_deref(), Some("0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11"));
    let hall = back.passage("Hall").unwrap();
    assert_eq!(hall.text, r#""Hello," says <b>Jo</b> & waves. [[Den]]"#);
    assert_eq!(hall.tags, ["cold", "dark"]);
    assert_eq!((hall.position.as_deref(), hall.size.as_deref()), (Some("600,400"), Some("200,100")));
    assert_eq!(back.passage("Den").unwrap().text, "The second den wins.");
    assert_eq!(back.passage("Den").unwrap().position.as_deref(), Some("225,100"), "the default place for pid 2");
    let script = &back.scripts[0].text;
    assert!(script.contains("var b = 2;") && script.contains("twine-user-script #2"), "{script}");
    assert_eq!(back.stylesheets[0].text, "body { color: red; }");
    assert_eq!(back.tag_colors.get("cold").map(String::as_str), Some("blue"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn refuses_without_an_ifid_and_offers_one() {
    let dir = scratch("no-ifid", &[("story.twee", ":: StoryTitle\nT\n\n:: StoryData\n{\"format\": \"SugarCube\"}\n\n:: Start\nHi.\n")]);
    let story = Story::read(&dir.join("story.twee")).unwrap();
    let error = compile(&story, &sugarcube(), &Options::default()).unwrap_err();
    assert!(error.contains("no IFID") && error.contains(":: StoryData") && error.contains("\"ifid\": \""), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn starts_where_asked_and_says_when_it_cant() {
    let dir = scratch(
        "start",
        &[("story.twee", ":: StoryTitle\nT\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\"}\n\n:: Start\nHi.\n\n:: Other\nThere.\n")],
    );
    let story = Story::read(&dir.join("story.twee")).unwrap();
    let page = compile(&story, &sugarcube(), &Options { start: Some("Other".into()), test: true, ..Options::default() }).unwrap();
    assert!(page.html.contains(r#"startnode="2""#) && page.html.contains(r#"options="debug""#));
    let error = compile(&story, &sugarcube(), &Options { start: Some("Nowhere".into()), ..Options::default() }).unwrap_err();
    assert!(error.contains("\"Nowhere\""), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn decompiles_to_twee_that_builds_the_same_story() {
    let dir = scratch("decompile", &[("story.twee", STORY), ("look.css", "::selection { color: red; }\nbody { margin: 0; }")]);
    let story = Story::read(&dir).unwrap();
    let page = compile(&story, &sugarcube(), &Options::default()).unwrap();
    std::fs::write(dir.join("first.html"), &page.html).unwrap();
    let published = Story::read(&dir.join("first.html")).unwrap();
    let twee = build::twee::story(&published);
    assert!(twee.contains("\n ::selection { color: red; }"), "a CSS line starting :: is indented so it can't read as a header: {twee}");
    assert!(build::twee::notes(&published).is_empty(), "{:?}", build::twee::notes(&published));
    let again_dir = dir.join("again");
    std::fs::create_dir_all(&again_dir).unwrap();
    std::fs::write(again_dir.join("story.twee"), &twee).unwrap();
    let again = compile(&Story::read(&again_dir.join("story.twee")).unwrap(), &sugarcube(), &Options::default()).unwrap();
    let body = |html: &str| html[html.find("<tw-passagedata").unwrap()..html.find("</tw-storydata>").unwrap()].to_string();
    assert_eq!(body(&again.html), body(&page.html), "the passages build back byte for byte");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn writes_archives_and_json() {
    let dir = scratch("archive", &[("story.twee", STORY)]);
    let story = Story::read(&dir.join("story.twee")).unwrap();
    let archive = build::output::archive(&story, &sugarcube(), &Options::default()).unwrap();
    assert!(archive.starts_with("<!-- UUID://0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11// --><tw-storydata"), "{}", &archive[..120]);
    assert!(archive.trim_end().ends_with("</tw-storydata>") && !archive.contains("<html"));
    let json: serde_json::Value = serde_json::from_str(&build::output::json(&story, &sugarcube(), &Options::default()).unwrap()).unwrap();
    assert_eq!(json["name"], "Tom & Jo's <Story>");
    assert_eq!(json["start"], "Hall");
    assert_eq!(json["format"], "SugarCube");
    assert!(json["passages"].as_array().unwrap().iter().any(|p| p["name"] == "Hall" && p["tags"][0] == "cold"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn fonts_media_and_modules() {
    let dir = scratch(
        "assets",
        &[
            ("src/story.twee", STORY),
            ("src/fonts/Grand.Hotel.ttf", "FONT"),
            ("src/img/bg.night.png", "PNG"),
            ("head/a.js", "var m = '</script>';"),
            ("head/b.css", "h1 {}"),
        ],
    );
    let mut story = Story::read(&dir.join("src")).unwrap();
    assert_eq!(build::sources::add_assets(&mut story, &[dir.join("src")], false), 0, "media stay files unless asked");
    let font = story.stylesheets.iter().find(|c| c.name == "Grand.Hotel.ttf").expect("the font as a stylesheet");
    assert_eq!(font.text, "@font-face {\n\tfont-family: \"Grand\";\n\tsrc: url(\"data:font/ttf;base64,Rk9OVA==\") format(\"truetype\");\n}");
    let mut embedded = Story::read(&dir.join("src")).unwrap();
    assert_eq!(build::sources::add_assets(&mut embedded, &[dir.join("src")], true), 1);
    let image = embedded.passage("bg").expect("named up to the first dot");
    assert_eq!((image.tags.clone(), image.text.clone()), (vec!["Twine.image".to_string()], "data:image/png;base64,UE5H".to_string()));
    let head = build::sources::modules(&[dir.join("head")]).unwrap();
    assert_eq!(
        head,
        [
            "<script id=\"script-module-a\" type=\"text/javascript\">var m = '<\\/script>';</script>",
            "<style id=\"style-module-b\" type=\"text/css\">h1 {}</style>"
        ]
    );
    let page = compile(&embedded, &sugarcube(), &Options { head, ..Options::default() }).unwrap();
    let modules_at = page.html.find("script-module-a").unwrap();
    assert!(modules_at < page.html.find("</head>").unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn reads_twee2_files() {
    let dir = scratch(
        "twee2",
        &[("story.tw2", ":: StoryTitle\nOld\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\"}\n\n:: Start [a] <300,200>\nHi.\n")],
    );
    let story = Story::read(&dir.join("story.tw2")).unwrap();
    assert_eq!(story.passage("Start").unwrap().position.as_deref(), Some("300,200"));
    std::fs::remove_dir_all(&dir).unwrap();
}
