//! Story formats: the four built in (SugarCube, Harlowe, Chapbook, Snowman), and any installed where Tweego looks for
//! them. A format is a folder named by its ID (`sugarcube-2`) holding `format.js`, the JSONP Twine loads:
//! `window.storyFormat({ name, version, source, … })`. It's read as a JavaScript object literal, so a format with
//! functions in it (Harlowe's `setup`) loads as well as a strict JSON one.

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Where a format came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    Folder(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Format {
    /// The folder's name: `sugarcube-2`.
    pub id: String,
    pub name: String,
    pub version: String,
    pub proofing: bool,
    /// The page, with `{{STORY_NAME}}` and `{{STORY_DATA}}` to fill in.
    pub source: String,
    pub origin: Origin,
}

/// The formats this program carries, and the notices that come with them.
pub const BUILT_IN: &[(&str, &str, &str)] = &[
    ("sugarcube-2", include_str!("../formats/sugarcube-2/format.js"), include_str!("../formats/sugarcube-2/LICENSE")),
    ("harlowe-3", include_str!("../formats/harlowe-3/format.js"), include_str!("../formats/harlowe-3/LICENSE")),
    ("chapbook-2", include_str!("../formats/chapbook-2/format.js"), include_str!("../formats/chapbook-2/LICENSE")),
    ("snowman-2", include_str!("../formats/snowman-2/format.js"), include_str!("../formats/snowman-2/LICENSE")),
];

/// What's in a `format.js`.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatData {
    pub name: Option<String>,
    pub version: String,
    pub proofing: bool,
    pub source: String,
}

/// Reads a `format.js`: from its first `{` to its last `}`, as a JavaScript object.
pub fn parse(text: &str) -> Result<FormatData, String> {
    let (Some(open), Some(close)) = (text.find('{'), text.rfind('}')) else {
        return Err("there's no `{ … }` object in it, as window.storyFormat({ … }) has".into());
    };
    let wrapped = format!("({});", &text[open..=close]);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &wrapped, SourceType::script()).parse();
    if let Some(error) = parsed.diagnostics.errors().next() {
        return Err(format!("its object doesn't parse: {}", error.message));
    }
    let Some(Statement::ExpressionStatement(statement)) = parsed.program.body.first() else {
        return Err("it doesn't hold an object".into());
    };
    let mut expression = &statement.expression;
    while let Expression::ParenthesizedExpression(inner) = expression {
        expression = &inner.expression;
    }
    let Expression::ObjectExpression(object) = expression else {
        return Err("it doesn't hold an object".into());
    };
    let (mut name, mut version, mut proofing, mut source) = (None, None, false, None);
    for property in &object.properties {
        let ObjectPropertyKind::ObjectProperty(property) = property else { continue };
        let key: String = match &property.key {
            PropertyKey::StaticIdentifier(id) => id.name.to_string(),
            PropertyKey::StaticStringLiteral(s) => s.value.display().to_string(),
            PropertyKey::StringLiteral(s) => s.value.display().to_string(),
            _ => continue,
        };
        let text = match &property.value {
            Expression::StringLiteral(s) => Some(s.value.display().to_string()),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => t.quasis.first().and_then(|q| q.value.cooked.map(|c| c.display().to_string())),
            _ => None,
        };
        let key = key.as_str();
        match (key, &property.value) {
            ("name", _) => name = text,
            ("version", _) => version = text,
            ("source", _) => source = text,
            ("proofing", Expression::BooleanLiteral(b)) => proofing = b.value,
            _ => {}
        }
    }
    let version: String = version.ok_or_else(|| "it has no version".to_string())?;
    if story::parse_version(&version).is_none() {
        return Err(format!("its version \"{version}\" isn't x.y.z"));
    }
    let source: String = source.ok_or_else(|| "it has no source".to_string())?;
    Ok(FormatData { name, version, proofing, source })
}

/// The folders Tweego looks in, in its order: beside this program, in the home folder, in the working folder, each
/// with `storyformats` (and the older names), then each folder in `TWEEGO_PATH`.
pub fn search_folders() -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = vec![];
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        bases.push(dir);
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        bases.push(PathBuf::from(home));
    }
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }
    bases.dedup();
    let mut out = vec![];
    for base in bases {
        for sub in ["storyformats", ".storyformats", "story-formats", "storyFormats", "targets"] {
            let dir = base.join(sub);
            if dir.is_dir() && !out.contains(&dir) {
                out.push(dir);
            }
        }
    }
    if let Some(paths) = std::env::var_os("TWEEGO_PATH") {
        out.extend(std::env::split_paths(&paths).filter(|p| p.is_dir()));
    }
    out
}

/// Every format: the built-in ones, then each found in the folders (one with the same ID replaces it), and a note for
/// each folder that couldn't be read.
pub fn all(folders: &[PathBuf]) -> (Vec<Format>, Vec<String>) {
    let mut formats: Vec<Format> = vec![];
    let mut notes = vec![];
    for (id, text, _) in BUILT_IN {
        match parse(text) {
            Ok(data) => formats.push(Format {
                id: id.to_string(),
                name: data.name.unwrap_or_else(|| id.to_string()),
                version: data.version,
                proofing: data.proofing,
                source: data.source,
                origin: Origin::BuiltIn,
            }),
            Err(error) => notes.push(format!("the built-in {id} couldn't be read: {error}")),
        }
    }
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else { continue };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            let file = dir.join("format.js");
            if !file.is_file() {
                continue;
            }
            let id = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            match std::fs::read_to_string(&file).map_err(|e| e.to_string()).and_then(|t| parse(&t)) {
                Ok(data) => {
                    formats.retain(|f| f.id != id);
                    formats.push(Format {
                        name: data.name.unwrap_or_else(|| id.clone()),
                        id,
                        version: data.version,
                        proofing: data.proofing,
                        source: data.source,
                        origin: Origin::Folder(dir),
                    });
                }
                Err(error) => notes.push(format!("{} was skipped: {error}", file.display())),
            }
        }
    }
    (formats, notes)
}

/// Picks the format for a story: one asked for (`sugarcube`, `SugarCube@2.36.1`, or an ID like `harlowe-3`), else the
/// story's own, else SugarCube. By name, it takes the same major version, the greatest at or above the one asked for;
/// with no version, the greatest of that name.
pub fn choose<'a>(formats: &'a [Format], asked: Option<&str>, story_format: Option<&str>, story_version: Option<&str>) -> Result<&'a Format, String> {
    let (name, version) = match asked {
        Some(asked) => match asked.split_once('@') {
            Some((name, version)) => (name.to_string(), Some(version.to_string())),
            None => (asked.to_string(), None),
        },
        None => match story_format {
            Some(name) => (name.to_string(), story_version.map(str::to_string)),
            None => ("SugarCube".to_string(), None),
        },
    };
    if let Some(by_id) = formats.iter().find(|f| f.id.eq_ignore_ascii_case(&name)) {
        return Ok(by_id);
    }
    let named: Vec<&Format> = formats.iter().filter(|f| f.name.eq_ignore_ascii_case(&name)).collect();
    let mut best: Option<&Format> = None;
    let wanted = version.as_deref().and_then(story::parse_version);
    for format in named {
        let Some(have) = story::parse_version(&format.version) else { continue };
        let fits = match wanted {
            Some(want) => have.0 == want.0 && have >= want,
            None => true,
        };
        if fits && best.is_none_or(|b| story::parse_version(&b.version).is_some_and(|v| have > v)) {
            best = Some(format);
        }
    }
    best.ok_or_else(|| {
        let what = match &version {
            Some(v) => format!("{name} {v} (or a later {}.x)", v.split('.').next().unwrap_or("")),
            None => name.clone(),
        };
        let have: Vec<String> = formats.iter().map(|f| format!("{} {} ({})", f.name, f.version, f.id)).collect();
        format!("There's no story format {what}. These are available: {}", have.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_built_in_format() {
        let (formats, notes) = all(&[]);
        assert!(notes.is_empty(), "{notes:?}");
        let found: Vec<(String, String)> = formats.iter().map(|f| (f.name.clone(), f.version.clone())).collect();
        assert_eq!(
            found,
            [("SugarCube", "2.37.3"), ("Harlowe", "3.3.9"), ("Chapbook", "2.3.1"), ("Snowman", "2.1.4")].map(|(n, v)| (n.to_string(), v.to_string()))
        );
        assert!(formats.iter().all(|f| f.source.contains("{{STORY_DATA}}")));
    }

    #[test]
    fn reads_object_literals_not_only_json() {
        let data = parse(
            r#"window.storyFormat({name: 'Tiny', version: "1.2.3", proofing: true, setup: function () { return 1 }, source: `<html>{{STORY_DATA}}</html>`});"#,
        )
        .unwrap();
        assert_eq!(data, FormatData { name: Some("Tiny".into()), version: "1.2.3".into(), proofing: true, source: "<html>{{STORY_DATA}}</html>".into() });
        assert!(parse("window.storyFormat({name: 'x'})").unwrap_err().contains("no version"));
        assert!(parse("nothing").is_err());
    }

    #[test]
    fn chooses_as_tweego_does() {
        let make = |id: &str, version: &str| Format {
            id: id.into(),
            name: "Story".into(),
            version: version.into(),
            proofing: false,
            source: String::new(),
            origin: Origin::BuiltIn,
        };
        let formats = vec![make("a", "1.0.0"), make("b", "2.0.0"), make("c", "2.5.0"), make("d", "3.0.0")];
        // The Tweego docs' example: 2.0.0 asked for, 1.0.0, 2.0.0, 2.5.0 and 3.0.0 installed: 2.5.0.
        assert_eq!(choose(&formats, None, Some("Story"), Some("2.0.0")).unwrap().version, "2.5.0");
        assert_eq!(choose(&formats, Some("story@1.0.0"), None, None).unwrap().version, "1.0.0");
        assert_eq!(choose(&formats, Some("story"), None, None).unwrap().version, "3.0.0");
        assert_eq!(choose(&formats, Some("b"), None, None).unwrap().id, "b");
        assert!(choose(&formats, None, Some("Story"), Some("4.0.0")).unwrap_err().contains("There's no story format Story 4.0.0"));
    }
}
