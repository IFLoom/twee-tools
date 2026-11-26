//! Checks of the story as a whole (IFID, start, format and version, title, published HTML), each on a small story made
//! for it in a temporary folder.

use std::path::PathBuf;

use check::{Severity, check, config::Config};
use story::Story;

/// Writes files into a new temporary folder and checks the story there; returns (code, severity, message, hint) for each
/// finding with one of the codes asked about.
fn findings(name: &str, files: &[(&str, &str)], codes: &[&str]) -> Vec<(String, Severity, String, Option<String>)> {
    let dir: PathBuf = std::env::temp_dir().join(format!("twee-tools-story-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let story = Story::read(&dir.join(files[0].0)).unwrap();
    let report = check(&story, &Config::default());
    std::fs::remove_dir_all(&dir).unwrap();
    report.findings.into_iter().filter(|f| codes.contains(&f.code)).map(|f| (f.code.to_string(), f.severity, f.message, f.hint)).collect()
}

const CODES: &[&str] = &["ifid", "start", "story-format", "format-version", "story-title", "story-data", "html-structure", "passage-name", "twine1-tags"];

fn twee(data: &str, rest: &str) -> String {
    format!(":: StoryTitle\nA Test\n\n:: StoryData\n{data}\n\n{rest}")
}

#[test]
fn a_good_story_has_nothing_to_say() {
    let text =
        twee(r#"{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "format": "SugarCube", "format-version": "2.37.3", "start": "Hall"}"#, ":: Hall\nHello.\n");
    assert_eq!(findings("good", &[("story.twee", &text)], CODES), vec![]);
}

#[test]
fn ifids() {
    let check = |ifid: &str| {
        findings(
            "ifid",
            &[("story.twee", &twee(&format!(r#"{{{ifid} "format": "SugarCube", "format-version": "2.37.3", "start": "Hall"}}"#), ":: Hall\nHi.\n"))],
            &["ifid"],
        )
    };
    let missing = check("");
    assert_eq!(missing.len(), 1);
    assert_eq!((missing[0].1, missing[0].2.contains("no IFID")), (Severity::Error, true));
    let lower = check(r#""ifid": "0c3b4b0e-6a33-4f0a-9e0a-6f1b5e2d7a11","#);
    assert_eq!(lower[0].3.as_deref(), Some("in capitals: 0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11"));
    let made_up = check(r#""ifid": "F6D9F39E-1234-5678-ABCD-1234567890AB","#);
    assert_eq!(made_up[0].1, Severity::Error, "the third group must start with 4");
    let example = check(r#""ifid": "D674C58C-DEFA-4F70-B7A2-27742230C0FC","#);
    assert_eq!((example[0].1, example[0].2.contains("examples")), (Severity::Warning, true));
}

#[test]
fn start_passages() {
    let none = findings(
        "start-none",
        &[("story.twee", &twee(r#"{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "format": "SugarCube", "format-version": "2.37.3"}"#, ":: Hall\nHi.\n"))],
        &["start"],
    );
    assert!(none.len() == 1 && none[0].2.contains("no start passage"), "{none:?}");
    let missing = findings(
        "start-missing",
        &[(
            "story.twee",
            &twee(r#"{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "format": "SugarCube", "format-version": "2.37.3", "start": "hall"}"#, ":: Hall\nHi.\n"),
        )],
        &["start"],
    );
    assert_eq!(missing[0].3.as_deref(), Some("did you mean \"Hall\"?"));
    let code = findings(
        "start-code",
        &[(
            "story.twee",
            &twee(
                r#"{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "format": "SugarCube", "format-version": "2.37.3", "start": "Tools"}"#,
                ":: Tools [widget]\n<<widget \"x\">>x<</widget>>\n",
            ),
        )],
        &["start"],
    );
    assert!(code[0].2.contains("tagged widget"), "{code:?}");
}

#[test]
fn formats_and_versions() {
    let with = |format: &str, rest: &str| {
        findings(
            "format",
            &[("story.twee", &twee(&format!(r#"{{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", {format} "start": "Hall"}}"#), rest))],
            &["story-format", "format-version"],
        )
    };
    let none = with("", ":: Hall\nHi.\n");
    assert!(none[0].2.contains("Harlowe"), "{none:?}");
    let cased = with(r#""format": "Sugarcube", "format-version": "2.37.3","#, ":: Hall\nHi.\n");
    assert_eq!(cased[0].3.as_deref(), Some("write \"SugarCube\""));
    let proofing = with(r#""format": "Paperthin", "format-version": "1.0.0","#, ":: Hall\nHi.\n");
    assert_eq!(proofing[0].1, Severity::Error);
    let twine1 = with(r#""format": "Sugarcane", "format-version": "1.0.0","#, ":: Hall\nHi.\n");
    assert_eq!(twine1[0].1, Severity::Warning);
    let version = with(r#""format": "SugarCube", "format-version": "2.37","#, ":: Hall\nHi.\n");
    assert!(version[0].2.contains("x.y.z"), "{version:?}");
    // <<silent>> came in 2.37.0; _args in a widget in 2.36.0; each reported once.
    let gated = with(
        r#""format": "SugarCube", "format-version": "2.30.0","#,
        ":: Hall\n<<silent>>a<</silent>><<silent>>b<</silent>>\n\n:: Tools [widget]\n<<widget \"x\">>_args[0]<</widget>>\n",
    );
    let features: Vec<&str> = gated.iter().filter(|f| f.0 == "format-version").map(|f| f.2.split(' ').next().unwrap()).collect();
    assert_eq!(features, ["<<silent>>", "_args"]);
    assert!(with(r#""format": "SugarCube", "format-version": "2.37.3","#, ":: Hall\n<<silent>>a<</silent>>\n").is_empty());
}

#[test]
fn titles_and_story_data() {
    let untitled = findings(
        "title",
        &[(
            "story.twee",
            ":: StoryTitle\nUntitled Story\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"SugarCube\", \"format-version\": \"2.37.3\", \"start\": \"Hall\"}\n\n:: Hall\nHi.\n",
        )],
        &["story-title"],
    );
    assert_eq!(untitled[0].1, Severity::Warning);
    let markup = findings(
        "title-markup",
        &[(
            "story.twee",
            ":: StoryTitle\n''Bold'' Title\n\n:: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"SugarCube\", \"format-version\": \"2.37.3\", \"start\": \"Hall\"}\n\n:: Hall\nHi.\n",
        )],
        &["story-title"],
    );
    assert!(markup[0].3.as_deref().unwrap().contains("StoryDisplayTitle"));
    let none = findings(
        "title-none",
        &[(
            "story.twee",
            ":: StoryData\n{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"SugarCube\", \"format-version\": \"2.37.3\", \"start\": \"Hall\"}\n\n:: Hall\nHi.\n",
        )],
        &["story-title"],
    );
    assert_eq!(none[0].1, Severity::Error);
    let data = findings(
        "data",
        &[(
            "story.twee",
            &twee(
                r#"{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "Format": "SugarCube", "format-version": "2.37.3", "start": "Hall", "zoom": "1", "tag-colors": {"a": "teal"}}"#,
                ":: Hall\nHi.\n",
            ),
        )],
        &["story-data"],
    );
    let messages: Vec<&str> = data.iter().map(|f| f.2.as_str()).collect();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert!(messages.iter().any(|m| m.contains("\"Format\"") && m.contains("did you mean \"format\"")));
    assert!(messages.iter().any(|m| m.contains("\"zoom\"")) && messages.iter().any(|m| m.contains("teal")));
    let broken = findings("data-broken", &[("story.twee", &twee(r#"{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11",}"#, ":: Hall\nHi.\n"))], CODES);
    assert_eq!(broken.len(), 1, "a StoryData that isn't JSON is one warning, not a cascade: {broken:?}");
}

#[test]
fn published_html() {
    let html = r#"<html><body><tw-storydata name="T" startnode="9" ifid="0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11" format="SugarCube" format-version="2.37.3" hidden>
<script type="text/twine-javascript">var a = 1;</script><script type="text/twine-javascript">var b = 2;</script>
<tw-passagedata pid="1" name="Hall" tags="">Hi.</tw-passagedata>
<tw-passagedata pid="1" name="Den " tags="">A <b>bold</b> word.</tw-passagedata>
<tw-passagedata name="Attic" tags="">Up.</tw-passagedata>
<tw-passagedata pid="7" name="Old Code" tags="script">setup.x = 1;</tw-passagedata>
</tw-storydata></body></html>"#;
    let found = findings("html", &[("game.html", html)], CODES);
    let has = |code: &str, text: &str| found.iter().any(|f| f.0 == code && f.2.contains(text));
    assert!(has("start", "pid 9"), "{found:#?}");
    assert!(has("html-structure", "the pid 1"), "{found:#?}");
    assert!(has("html-structure", "no pid"), "{found:#?}");
    assert!(has("html-structure", "elements inside"), "{found:#?}");
    assert!(has("html-structure", "2 story JavaScript"), "{found:#?}");
    assert!(has("passage-name", "starts or ends with a space"), "{found:#?}");
    assert!(has("twine1-tags", "its code never runs"), "{found:#?}");
}
