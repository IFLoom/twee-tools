use std::collections::BTreeMap;
use std::path::Path;

use check::{Severity, check, config::Config};
use story::Story;

/// The seeded story has exactly one of each problem, and the things that look like problems and aren't (a widget, a macro the
/// JavaScript adds, a variable set from JavaScript, links worked out at runtime, links in comments, <<endif>>) go unreported.
#[test]
fn finds_each_seeded_problem_once() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/seeded");
    let story = Story::read(&dir.join("story.twee")).unwrap();
    let report = check(&story, &Config::default());
    let mut counts: BTreeMap<(&str, Severity), usize> = BTreeMap::new();
    for finding in &report.findings {
        *counts.entry((finding.code, finding.severity)).or_default() += 1;
    }
    let expected = BTreeMap::from([
        (("broken-link", Severity::Error), 2),
        (("duplicate-passage", Severity::Error), 1),
        (("misplaced-macro", Severity::Error), 1),
        (("missing-media", Severity::Error), 1),
        (("missing-media", Severity::Warning), 2),
        (("never-read", Severity::Note), 1),
        (("never-set", Severity::Warning), 2),
        (("stray-close", Severity::Error), 1),
        (("unclosed-macro", Severity::Error), 1),
        (("unknown-macro", Severity::Error), 1),
        (("misnamed-media", Severity::Warning), 1),
        (("unreachable", Severity::Warning), 1),
        (("dead-end", Severity::Warning), 1),
        (("unwritten", Severity::Warning), 1),
        (("unused-macro", Severity::Warning), 2),
        (("widget-as-function", Severity::Error), 1),
        (("unclosed-markup", Severity::Error), 2),
        (("js-syntax", Severity::Error), 2),
        (("twee-header", Severity::Warning), 3),
        (("passage-name", Severity::Warning), 1),
        (("special-name", Severity::Error), 1),
        (("code-passage", Severity::Error), 1),
        (("no-output", Severity::Warning), 1),
        (("removed-api", Severity::Error), 1),
        (("deprecated", Severity::Note), 1),
        (("macro-name", Severity::Error), 1),
        (("config", Severity::Error), 1),
        (("if-assignment", Severity::Error), 1),
        (("receiver", Severity::Error), 1),
        (("macro-args", Severity::Error), 2),
        (("switch", Severity::Error), 1),
        (("variable-name", Severity::Error), 1),
        (("naked-variable", Severity::Warning), 1),
        (("def-precedence", Severity::Note), 1),
        (("goto-continues", Severity::Warning), 2),
        (("capture", Severity::Error), 1),
        (("audio", Severity::Error), 1),
        (("link-target", Severity::Warning), 1),
        (("wrong-format", Severity::Error), 1),
        (("absolute-media", Severity::Error), 1),
        (("unused-media", Severity::Warning), 2),
    ]);
    let listing: Vec<String> =
        report.findings.iter().map(|f| format!("{}:{} {} {}", f.line, f.passage.clone().unwrap_or_default(), f.code, f.message)).collect();
    assert_eq!(counts, expected, "{listing:#?}");
    let by = |code: &str| report.findings.iter().find(|f| f.code == code).unwrap();
    assert!(by("broken-link").message.contains("Nowhere"));
    assert!(by("never-set").message.contains("$ghost"));
    assert!(by("never-read").message.contains("$unused"));
    assert!(by("unknown-macro").message.contains("shout"));
    assert!(by("unreachable").message.contains("Attic"));
    assert!(report.findings.iter().any(|f| f.code == "unused-media" && f.message.contains("spare.png")));
    assert_eq!(by("broken-link").line, 17);
    // A missing file a typo away from one in its folder is suggested.
    let missing = report.findings.iter().find(|f| f.code == "missing-media" && f.severity == Severity::Error).unwrap();
    assert_eq!(missing.hint.as_deref(), Some("did you mean \"media/gane.png\"?"));
    assert_eq!(missing.subject.as_deref(), Some("media/gone.png"));
    assert_eq!(by("never-set").subject.as_deref(), Some("$ghost"));
    let property = report.findings.iter().find(|f| f.subject.as_deref() == Some("$office.reputaton")).expect("the misspelt property");
    assert_eq!(property.hint.as_deref(), Some("did you mean $office.reputation?"));
    assert_eq!(report.errors(), 28);
    assert_eq!(by("absolute-media").hint.as_deref(), Some("write it relative to the story: \"media/shop.png\""));
    assert_eq!(by("js-syntax").subject.as_deref(), Some("<<if>>"));
    assert_eq!(by("config").hint.as_deref(), Some("did you mean Config.passages.start?"));
    assert_eq!(by("no-output").hint.as_deref(), Some("`//` isn't a comment in SugarCube's markup; write /* … */"));
    assert_eq!(by("removed-api").subject.as_deref(), Some("is not"));
    assert_eq!(by("special-name").hint.as_deref(), Some("tag it nobr"));
    let headers: Vec<_> = report.findings.iter().filter(|f| f.code == "twee-header").map(|f| f.message.as_str()).collect();
    assert!(
        headers.iter().any(|m| m.contains("indented")) && headers.iter().any(|m| m.contains("twice")) && headers.iter().any(|m| m.contains("\"position\"")),
        "{headers:?}"
    );
    assert_eq!(by("dead-end").subject.as_deref(), Some("Piercing Ear"));
    assert_eq!(by("unwritten").subject.as_deref(), Some("Notes"));
    assert_eq!(by("widget-as-function").subject.as_deref(), Some("tally(…)"));
    let unused: Vec<_> = report.findings.iter().filter(|f| f.code == "unused-macro").filter_map(|f| f.subject.clone()).collect();
    assert_eq!(unused, ["<<spare>>", "<<tally>>"]);
}

/// The media summary weighs files by what they really are, and a file of 5 MB or more is reported.
#[test]
fn weighs_the_media_and_reports_large_files() {
    let dir = std::env::temp_dir().join(format!("twee-tools-weight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("media")).unwrap();
    std::fs::write(
        dir.join("story.twee"),
        ":: StoryData\n{\"format\": \"SugarCube\", \"start\": \"Hall\"}\n\n:: Hall\n<img src=\"media/big.gif\"><img src=\"media/small.jpg\">\n",
    )
    .unwrap();
    // A big animated GIF (a GIF header, then 6 MB, written sparse), and a small JPEG named .jpg that's really a PNG.
    let big = std::fs::File::create(dir.join("media/big.gif")).unwrap();
    std::io::Write::write_all(&mut &big, b"GIF89a").unwrap();
    big.set_len(6 * 1024 * 1024).unwrap();
    std::fs::write(dir.join("media/small.jpg"), [0x89, b'P', b'N', b'G', 0, 0, 0, 0]).unwrap();
    let story = Story::read(&dir.join("story.twee")).unwrap();
    let report = check(&story, &Config::default());
    assert_eq!(report.media.files, 2);
    assert_eq!(report.media.bytes, 6 * 1024 * 1024 + 8);
    assert_eq!(report.media.kinds[0].0, "GIF");
    assert_eq!(report.media.compressible_bytes, 6 * 1024 * 1024 + 8);
    let large: Vec<_> = report.findings.iter().filter(|f| f.code == "large-media").collect();
    assert_eq!(large.len(), 1);
    assert_eq!((large[0].subject.as_deref(), large[0].bytes, large[0].hint.as_deref()), (Some("media/big.gif"), Some(6 * 1024 * 1024), Some("GIF")));
    assert!(report.findings.iter().any(|f| f.code == "misnamed-media" && f.subject.as_deref() == Some("media/small.jpg")));
    std::fs::remove_dir_all(&dir).unwrap();
}
