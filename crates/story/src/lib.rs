//! A Twine story, read from Twee 3 source or from the HTML Twine publishes, as one model.

pub mod markup;
pub mod media;

/// What a long job says it's doing, for a spinner: "Checking media 120 of 2570".
pub type Progress<'a> = &'a (dyn Fn(&str) + Sync);

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use scraper::{Html, Selector};
use serde::Serialize;

/// Where a story was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Twee,
    Html,
}

/// One passage: its name, tags and text, and where its header is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Passage {
    pub name: String,
    pub tags: Vec<String>,
    pub text: String,
    pub file: PathBuf,
    /// The line of the passage's header (Twee) or its `<tw-passagedata>` (HTML), from 1.
    pub line: usize,
    /// The line the passage's text starts on.
    pub body_line: usize,
    /// Where Twine's map shows it, "x,y", and its size, "w,h", from Twee metadata or the published HTML.
    pub position: Option<String>,
    pub size: Option<String>,
}

impl Passage {
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// The line of a byte offset in the passage's text.
    pub fn line_at(&self, offset: usize) -> usize {
        self.body_line + self.text[..offset.min(self.text.len())].matches('\n').count()
    }
}

/// A piece of the story's own JavaScript or CSS, with the file it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Code {
    /// The passage or file it came from: `Story JavaScript`, `ui.js`.
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Story {
    pub form: Form,
    /// The folder the story was read from: the HTML file's, or the Twee folder.
    pub root: PathBuf,
    /// The folder the page's media paths are relative to: where the page is, or (for Twee) where it will be built.
    /// The same as `root` unless a build says otherwise.
    pub media_root: PathBuf,
    /// Files and folders that will be beside the page but aren't yet, as a build's assets: each one on disk, and the
    /// path it will have under `media_root` (`src/media` as `media`). Media found there count as found.
    pub media_mounts: Vec<(PathBuf, String)>,
    pub title: Option<String>,
    pub format: Option<String>,
    pub format_version: Option<String>,
    pub start: Option<String>,
    pub passages: Vec<Passage>,
    pub scripts: Vec<Code>,
    pub stylesheets: Vec<Code>,
    /// The Twee files read, for a story read from Twee.
    pub files: Vec<PathBuf>,
    pub ifid: Option<String>,
    /// Where the story's data is: the StoryData passage, or `<tw-storydata>`.
    pub data_at: Option<(PathBuf, usize)>,
    /// The StoryTitle passage as written, and where (Twee).
    pub title_passage: Option<(String, PathBuf, usize)>,
    /// What reading the story noticed about its structure, for the checker to report.
    pub problems: Vec<Problem>,
    /// Each passage declared again after its first declaration: the story keeps the last one, in the first one's place,
    /// as Tweego builds it and SugarCube and Harlowe play a page that has two.
    pub duplicates: Vec<Duplicate>,
    /// Tag colours, from StoryData's `tag-colors` or `<tw-tag>`; Twine's map zoom.
    pub tag_colors: std::collections::BTreeMap<String, String>,
    pub zoom: Option<f64>,
}

/// Something reading a story noticed that the specs say is wrong: a bad header, StoryData that isn't JSON, a missing pid.
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    /// The checker's code for it: `story-data`, `twee-header`, `html-structure`, `start`, `passage-name`.
    pub kind: &'static str,
    pub error: bool,
    pub message: String,
    pub file: PathBuf,
    pub line: usize,
    pub passage: Option<String>,
}

fn problem(kind: &'static str, error: bool, message: impl Into<String>, file: &Path, line: usize, passage: Option<&str>) -> Problem {
    Problem { kind, error, message: message.into(), file: file.to_path_buf(), line, passage: passage.map(str::to_string) }
}

/// The seven colours a tag can have (Twine 2 HTML output spec).
pub const TAG_COLORS: &[&str] = &["gray", "red", "orange", "yellow", "green", "blue", "purple"];

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn error(message: impl Into<String>) -> Error {
    Error(message.into())
}

impl Story {
    /// Reads a story from a published `.html` file, a `.twee`/`.tw` file, or a folder of Twee files.
    pub fn read(path: &Path) -> Result<Story, Error> {
        if path.is_dir() {
            return read_twee_dir(path);
        }
        match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("html" | "htm") => {
                let html = fs::read_to_string(path).map_err(|e| error(format!("{}: {e}", path.display())))?;
                read_html(&html, path)
            }
            Some("twee" | "tw" | "twee2" | "tw2") => {
                let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                let mut story = empty(Form::Twee, root);
                add_twee_file(&mut story, path)?;
                finish_twee(story)
            }
            _ => Err(error(format!("{}: expected a .html story, a .twee file or a folder of them", path.display()))),
        }
    }

    /// Reads one story from several paths, as Tweego does: Twee files and folders of them, read together. A published
    /// .html story stands alone. Media paths are relative to the first path (its folder, if it's a file).
    pub fn read_all(paths: &[PathBuf]) -> Result<Story, Error> {
        Story::read_all_with(paths, &[])
    }

    /// `read_all`, leaving out files that match any of the globs (`src/drafts/**`, `**/*.old.twee`), as a project file
    /// excludes them.
    pub fn read_all_with(paths: &[PathBuf], exclude: &[String]) -> Result<Story, Error> {
        let globs = glob_set(exclude)?;
        let exclude = (!exclude.is_empty()).then_some(&globs);
        match paths {
            [] => Err(error("Give a story: a .html file, a .twee file, or a folder of them.")),
            [one] if one.is_dir() => read_twee_dir_with(one, exclude),
            [one] => Story::read(one),
            many => {
                if let Some(html) = many.iter().find(|p| p.is_file() && !is_twee(p)) {
                    return Err(error(format!("{}: only Twee files and folders can be read together; check a published story on its own", html.display())));
                }
                let first = &many[0];
                let root = if first.is_dir() { first.clone() } else { first.parent().unwrap_or(Path::new(".")).to_path_buf() };
                let mut story = empty(Form::Twee, root);
                let mut seen = std::collections::HashSet::new();
                for path in many {
                    add_twee_path(&mut story, path, &mut seen, exclude)?;
                }
                finish_twee(story)
            }
        }
    }

    /// A story from Twee text, read as if it were `file` (which needn't exist; media paths are relative to its folder):
    /// for tests, and for tools that hold the text already.
    pub fn from_twee(text: &str, file: &Path) -> Result<Story, Error> {
        let mut story = empty(Form::Twee, file.parent().unwrap_or(Path::new("")).to_path_buf());
        add_twee_text(&mut story, text, file);
        finish_twee(story)
    }

    /// Where a media path (relative to `media_root`) is on disk: under a mount if it starts with one, else under the root.
    pub fn media_path(&self, rel: &str) -> PathBuf {
        media::path_of(&self.media_root, &self.media_mounts, rel)
    }

    pub fn passage(&self, name: &str) -> Option<&Passage> {
        self.passages.iter().find(|p| p.name == name)
    }

    /// Whether the story's format is SugarCube (any version).
    pub fn is_sugarcube(&self) -> bool {
        self.format.as_deref().is_some_and(|f| f.eq_ignore_ascii_case("sugarcube"))
    }
}

fn empty(form: Form, root: PathBuf) -> Story {
    Story {
        form,
        media_root: root.clone(),
        media_mounts: vec![],
        root,
        title: None,
        format: None,
        format_version: None,
        start: None,
        passages: vec![],
        scripts: vec![],
        stylesheets: vec![],
        files: vec![],
        ifid: None,
        data_at: None,
        title_passage: None,
        problems: vec![],
        duplicates: vec![],
        tag_colors: Default::default(),
        zoom: None,
    }
}

/// A passage declared more than once: where this later declaration is, and where the first one was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Duplicate {
    pub name: String,
    pub file: PathBuf,
    pub line: usize,
    pub first_file: PathBuf,
    pub first_line: usize,
}

/// One passage per name: a later declaration replaces the earlier one where it stood, and is recorded.
fn merge_duplicates(story: &mut Story) {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut kept: Vec<Passage> = vec![];
    for passage in std::mem::take(&mut story.passages) {
        match at.get(&passage.name) {
            Some(&index) => {
                let (first_file, first_line) = story
                    .duplicates
                    .iter()
                    .find(|d| d.name == passage.name)
                    .map(|d| (d.first_file.clone(), d.first_line))
                    .unwrap_or_else(|| (kept[index].file.clone(), kept[index].line));
                story.duplicates.push(Duplicate { name: passage.name.clone(), file: passage.file.clone(), line: passage.line, first_file, first_line });
                kept[index] = passage;
            }
            None => {
                at.insert(passage.name.clone(), kept.len());
                kept.push(passage);
            }
        }
    }
    story.passages = kept;
}

/// A story format's version, x.y.z (anything after `-` or `+` in a part is left off): what StoryData's
/// `format-version` and a format's `version` hold.
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = text.trim().split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let number = |p: &str| p.split(['-', '+']).next().and_then(|n| n.parse::<u32>().ok());
    Some((number(parts[0])?, number(parts[1])?, number(parts[2])?))
}

/// Folders no tool reads into a story or copies out of one: hidden ones (`.git`), `node_modules` and `target`.
pub fn skipped_folder(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name == "target"
}

/// A file or folder a walk over a story's files passes by: anything hidden, and the skipped folders.
pub fn passed_by(name: &str) -> bool {
    name.starts_with('.') || skipped_folder(name)
}

// ---- HTML ----

/// Reads a story Twine (or Tweego) published: `<tw-storydata>` and its `<tw-passagedata>`.
pub fn read_html(html: &str, file: &Path) -> Result<Story, Error> {
    let root = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let doc = Html::parse_document(html);
    let data_sel = Selector::parse("tw-storydata").unwrap();
    let data = doc.select(&data_sel).next().ok_or_else(|| error(format!("{}: no <tw-storydata>, so it isn't a Twine 2 story", file.display())))?;
    let attr = |name: &str| data.value().attr(name).map(str::to_string).filter(|v| !v.is_empty());
    let mut story = empty(Form::Html, root);
    story.title = attr("name");
    story.format = attr("format");
    story.format_version = attr("format-version");
    story.ifid = attr("ifid");
    story.zoom = attr("zoom").and_then(|z| z.parse().ok());
    let tag_sel = Selector::parse("tw-tag").unwrap();
    for el in data.select(&tag_sel) {
        if let (Some(name), Some(color)) = (el.value().attr("name"), el.value().attr("color")) {
            story.tag_colors.insert(name.to_string(), color.to_string());
        }
    }
    let start_pid = attr("startnode");
    let data_line = lines_of(html, "<tw-storydata").first().copied().unwrap_or(1);
    story.data_at = Some((file.to_path_buf(), data_line));
    let stories = doc.select(&data_sel).count();
    if stories > 1 {
        story.problems.push(problem(
            "html-structure",
            false,
            format!("The file holds {stories} stories, as a Twine archive does; only the first is checked"),
            file,
            data_line,
            None,
        ));
    }

    // scraper keeps no positions, so the lines come from the raw text, in document order.
    let tag_lines = lines_of(html, "<tw-passagedata");
    let passage_sel = Selector::parse("tw-passagedata").unwrap();
    let mut pids: HashMap<String, String> = HashMap::new();
    for (at, el) in data.select(&passage_sel).enumerate() {
        let name = el.value().attr("name").unwrap_or_default().to_string();
        let tags = el.value().attr("tags").unwrap_or_default().split_whitespace().map(str::to_string).collect();
        let text: String = el.text().collect();
        let line = tag_lines.get(at).copied().unwrap_or(1);
        match el.value().attr("pid").filter(|p| !p.is_empty()) {
            None => story.problems.push(problem("html-structure", true, format!("\"{name}\" has no pid"), file, line, Some(&name))),
            Some(pid) => {
                if let Some(other) = pids.insert(pid.to_string(), name.clone()) {
                    story.problems.push(problem("html-structure", true, format!("\"{name}\" has the pid {pid}, as \"{other}\" does"), file, line, Some(&name)));
                }
            }
        }
        if el.children().any(|c| c.value().is_element()) {
            story.problems.push(problem(
                "html-structure",
                true,
                format!("\"{name}\" has HTML elements inside <tw-passagedata>, which must hold only text: its < and & weren't escaped"),
                file,
                line,
                Some(&name),
            ));
        }
        if name.trim() != name {
            story.problems.push(problem(
                "passage-name",
                false,
                format!("\"{name}\" starts or ends with a space, so a link to \"{}\" doesn't reach it", name.trim()),
                file,
                line,
                Some(&name),
            ));
        }
        if start_pid.is_some() && el.value().attr("pid").map(str::to_string) == start_pid {
            story.start = Some(name.clone());
        }
        let position = el.value().attr("position").map(str::to_string).filter(|v| !v.is_empty());
        let size = el.value().attr("size").map(str::to_string).filter(|v| !v.is_empty());
        story.passages.push(Passage { name, tags, text: trim_end(&text), file: file.to_path_buf(), line, body_line: line, position, size });
    }
    match &start_pid {
        Some(pid) if story.start.is_none() => {
            story.problems.push(problem("start", true, format!("The story starts at pid {pid}, and no passage has that pid"), file, data_line, None));
        }
        _ => {}
    }
    for (selector, into_scripts) in
        [("script#twine-user-script, script[type='text/twine-javascript']", true), ("style#twine-user-stylesheet, style[type='text/twine-css']", false)]
    {
        let sel = Selector::parse(selector).unwrap();
        let marker = if into_scripts { "twine-user-script" } else { "twine-user-stylesheet" };
        let line = lines_of(html, marker).first().copied().unwrap_or(1);
        let count = data.select(&sel).count();
        if count > 1 {
            let what = if into_scripts { "story JavaScript" } else { "story stylesheet" };
            story.problems.push(problem("html-structure", false, format!("There are {count} {what} elements; Twine writes one"), file, line, None));
        }
        for el in data.select(&sel) {
            let name = if into_scripts { "Story JavaScript" } else { "Story Stylesheet" }.to_string();
            let code = Code { name, file: file.to_path_buf(), line, text: el.text().collect() };
            if into_scripts { story.scripts.push(code) } else { story.stylesheets.push(code) }
        }
    }
    merge_duplicates(&mut story);
    Ok(story)
}

fn lines_of(text: &str, needle: &str) -> Vec<usize> {
    let mut lines = vec![];
    let mut line = 1;
    let mut last = 0;
    for (at, _) in text.match_indices(needle) {
        line += text[last..at].matches('\n').count();
        last = at;
        lines.push(line);
    }
    lines
}

fn trim_end(text: &str) -> String {
    text.trim_end().to_string()
}

// ---- Twee 3 ----

fn is_twee(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["twee", "tw", "twee2", "tw2"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Globs, for leaving files out.
pub fn glob_set(globs: &[String]) -> Result<globset::GlobSet, Error> {
    let mut set = globset::GlobSetBuilder::new();
    for glob in globs {
        set.add(globset::Glob::new(glob).map_err(|e| error(format!("The exclude pattern \"{glob}\" isn't a glob: {e}")))?);
    }
    set.build().map_err(|e| error(e.to_string()))
}

/// Whether a file is left out: its path as given, or without a leading `./`.
pub fn excluded(file: &Path, exclude: Option<&globset::GlobSet>) -> bool {
    exclude.is_some_and(|set| set.is_match(file) || file.strip_prefix(".").is_ok_and(|p| set.is_match(p)))
}

fn read_twee_dir(dir: &Path) -> Result<Story, Error> {
    read_twee_dir_with(dir, None)
}

fn read_twee_dir_with(dir: &Path, exclude: Option<&globset::GlobSet>) -> Result<Story, Error> {
    let mut story = empty(Form::Twee, dir.to_path_buf());
    add_twee_path(&mut story, dir, &mut std::collections::HashSet::new(), exclude)?;
    if !story.files.is_empty() {
        return finish_twee(story);
    }
    // No Twee here: a published story's folder, the HTML beside its media. Check that, if there's just one.
    let mut published: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| error(format!("{}: {e}", dir.display())))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm")))
        .filter(|p| fs::read_to_string(p).is_ok_and(|t| t.contains("<tw-storydata")))
        .collect();
    published.sort();
    match published.as_slice() {
        [one] => Story::read(one),
        [] => Err(error(format!("{}: no Twee files (.twee, .tw) and no published Twine story (.html) here", dir.display()))),
        many => Err(error(format!(
            "{}: no Twee files, and {} published stories; check one of them:\n{}",
            dir.display(),
            many.len(),
            many.iter().map(|p| format!("  {}", p.display())).collect::<Vec<_>>().join("\n")
        ))),
    }
}

/// Adds a Twee file, or every Twee, JavaScript and CSS file in a folder and below it, as Tweego does. Each file once.
fn add_twee_path(story: &mut Story, path: &Path, seen: &mut std::collections::HashSet<PathBuf>, exclude: Option<&globset::GlobSet>) -> Result<(), Error> {
    let mut files = vec![];
    if path.is_dir() {
        walk(path, &mut files).map_err(|e| error(format!("{}: {e}", path.display())))?;
        files.sort();
    } else if path.is_file() {
        files.push(path.to_path_buf());
    } else {
        return Err(error(format!("{}: no such file or folder", path.display())));
    }
    for file in files {
        let ext = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("twee" | "tw" | "twee2" | "tw2" | "js" | "css")) || excluded(&file, exclude) {
            continue;
        }
        // Only the story's own files are resolved (each a slow call on a Windows drive under WSL), not every image beside them.
        if !seen.insert(fs::canonicalize(&file).unwrap_or_else(|_| file.clone())) {
            continue;
        }
        match ext.as_deref() {
            Some("twee" | "tw" | "twee2" | "tw2") => add_twee_file(story, &file)?,
            // Tweego adds a folder's .js and .css to the story's script and stylesheet.
            Some("js") => story.scripts.push(Code { name: base_name(&file), text: read(&file)?, file, line: 1 }),
            Some("css") => story.stylesheets.push(Code { name: base_name(&file), text: read(&file)?, file, line: 1 }),
            _ => {}
        }
    }
    Ok(())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let hidden = path.file_name().and_then(|n| n.to_str()).is_some_and(passed_by);
        if hidden {
            continue;
        }
        // The listing's own file type, not a stat per file: on a Windows drive under WSL each stat is a slow round trip.
        let is_dir = entry.file_type().map(|t| t.is_dir() || (t.is_symlink() && path.is_dir())).unwrap_or(false);
        // A folder below that can't be read is skipped; only the one asked for must be readable.
        if is_dir {
            let _ = walk(&path, out);
        } else {
            out.push(path)
        }
    }
    Ok(())
}

/// Twee2's `:: Name [tags] <x,y>` headers as Twee 3's `:: Name [tags] {"position":"x,y"}`.
pub fn from_twee2(text: &str) -> String {
    text.lines()
        .map(|line| {
            let Some(header) = line.strip_prefix("::") else { return line.to_string() };
            let trimmed = header.trim_end();
            match (trimmed.ends_with('>'), trimmed.rfind('<')) {
                (true, Some(open)) => {
                    let place = trimmed[open + 1..trimmed.len() - 1].trim();
                    let head = trimmed[..open].trim_end();
                    if place.is_empty() { format!("::{head}") } else { format!("::{head} {{\"position\":\"{place}\"}}") }
                }
                _ => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn base_name(file: &Path) -> String {
    file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

fn read(file: &Path) -> Result<String, Error> {
    fs::read_to_string(file).map_err(|e| error(format!("{}: {e}", file.display())))
}

fn add_twee_file(story: &mut Story, file: &Path) -> Result<(), Error> {
    let text = read(file)?;
    // Twee2 writes a passage's place as `:: Name [tags] <x,y>`: Twee 3 metadata.
    let twee2 = file.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("twee2") || e.eq_ignore_ascii_case("tw2"));
    let text = if twee2 { from_twee2(&text) } else { text };
    add_twee_text(story, &text, file);
    Ok(())
}

fn add_twee_text(story: &mut Story, text: &str, file: &Path) {
    story.files.push(file.to_path_buf());
    let (passages, problems) = parse_twee_checked(text, file);
    story.passages.extend(passages);
    story.problems.extend(problems);
}

/// The passages in one Twee 3 file: each starts at a line beginning `::`.
pub fn parse_twee(text: &str, file: &Path) -> Vec<Passage> {
    parse_twee_checked(text, file).0
}

/// The passages in one Twee 3 file, and what the Twee 3 spec says is wrong with their headers.
pub fn parse_twee_checked(text: &str, file: &Path) -> (Vec<Passage>, Vec<Problem>) {
    let mut problems = vec![];
    let mut passages: Vec<Passage> = vec![];
    let mut body: Vec<&str> = vec![];
    let flush = |passages: &mut Vec<Passage>, body: &mut Vec<&str>| {
        if let Some(last) = passages.last_mut() {
            last.text = trim_end(&body.join("\n"));
        }
        body.clear();
    };
    for (at, line) in text.lines().enumerate() {
        if let Some(header) = line.strip_prefix("::") {
            flush(&mut passages, &mut body);
            let (name, tags, wrong, meta) = parse_header(header);
            for message in wrong {
                problems.push(problem("twee-header", false, message, file, at + 1, Some(&name)));
            }
            let field = |key: &str| meta.get(key).and_then(|v| v.as_str()).map(str::to_string);
            let (position, size) = (field("position"), field("size"));
            passages.push(Passage { name, tags, text: String::new(), file: file.to_path_buf(), line: at + 1, body_line: at + 2, position, size });
        } else {
            if line.trim_start().starts_with("::") && line.trim_start().len() > 2 {
                problems.push(problem(
                    "twee-header",
                    false,
                    "A passage header indented: `::` must begin the line, so this one is read as text of the passage before",
                    file,
                    at + 1,
                    None,
                ));
            }
            if !passages.is_empty() {
                body.push(line);
            }
        }
    }
    flush(&mut passages, &mut body);
    (passages, problems)
}

/// A header after its `::`: a name with `\`-escapes, then optional `[tags]` and `{metadata}`; and what's wrong with it.
fn parse_header(header: &str) -> (String, Vec<String>, Vec<String>, HashMap<String, serde_json::Value>) {
    // Each character, and whether it was escaped: `:: \ Hall` keeps its leading space, which trimming must not take.
    let mut name: Vec<(char, bool)> = vec![];
    let mut chars = header.trim_start().chars().peekable();
    let mut rest = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    name.push((next, true))
                }
            }
            '[' | '{' => {
                rest.push(c);
                rest.extend(chars.by_ref());
                break;
            }
            _ => name.push((c, false)),
        }
    }
    let plain_space = |&(c, escaped): &(char, bool)| c.is_whitespace() && !escaped;
    let from = name.iter().position(|c| !plain_space(c)).unwrap_or(name.len());
    let to = name.iter().rposition(|c| !plain_space(c)).map_or(from, |i| i + 1);
    let name: String = name[from..to].iter().map(|(c, _)| c).collect();
    let mut wrong = vec![];
    let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();
    let tags: Vec<String> = match (rest.starts_with('['), rest.find(']')) {
        (true, Some(end)) => rest[1..end].split_whitespace().map(str::to_string).collect(),
        _ => vec![],
    };
    let mut seen = std::collections::HashSet::new();
    for tag in &tags {
        if !seen.insert(tag) {
            wrong.push(format!("The tag \"{tag}\" is given twice"));
        }
    }
    // Tags come straight after the name, then the metadata: `:: Name [tags] {"position":"1,1"}`.
    let after_tags = match (rest.starts_with('['), rest.find(']')) {
        (true, Some(end)) => rest[end + 1..].trim(),
        _ => rest.trim(),
    };
    if after_tags.starts_with('{') {
        let block = after_tags.to_string();
        if let Some(late) = block.rfind('}').map(|end| block[end + 1..].trim().to_string()).filter(|t| t.starts_with('[')) {
            let _ = late;
            wrong.push("The tags come after the metadata; they must come straight after the name".into());
        }
        let json = &block[..block.rfind('}').map(|e| e + 1).unwrap_or(block.len())];
        match serde_json::from_str::<HashMap<String, serde_json::Value>>(json) {
            Err(_) => wrong.push(format!("The metadata {json} isn't JSON, so it's dropped")),
            Ok(meta) => {
                metadata = meta.clone();
                for key in ["position", "size"] {
                    if let Some(value) = meta.get(key) {
                        let ok = value.as_str().is_some_and(|v| {
                            let parts: Vec<&str> = v.split(',').collect();
                            parts.len() == 2 && parts.iter().all(|p| p.trim().parse::<f64>().is_ok())
                        });
                        if !ok {
                            wrong.push(format!("The metadata's \"{key}\" is {value}; it should be a string of two numbers, \"x,y\""));
                        }
                    }
                }
            }
        }
    } else if rest.starts_with('{') && rest.contains('[') {
        wrong.push("The tags come after the metadata; they must come straight after the name".into());
    }
    (name, tags, wrong, metadata)
}

/// StoryData's keys, as the Twee 3 spec gives them. Bad JSON is a warning, as the spec says: drop it and go on.
fn read_story_data(story: &mut Story, passage: &Passage) {
    let (file, line) = (&passage.file, passage.line);
    let data: HashMap<String, serde_json::Value> = match serde_json::from_str(&passage.text) {
        Ok(data) => data,
        Err(e) => {
            story.problems.push(problem(
                "story-data",
                false,
                format!("StoryData isn't JSON ({e}), so its IFID, format and start are lost"),
                file,
                line,
                Some("StoryData"),
            ));
            return;
        }
    };
    let text = |key: &str| data.get(key).and_then(|v| v.as_str()).map(str::to_string);
    story.format = text("format");
    story.format_version = text("format-version");
    story.start = text("start");
    story.ifid = text("ifid");
    story.zoom = data.get("zoom").and_then(|v| v.as_f64());
    if let Some(colors) = data.get("tag-colors").and_then(|v| v.as_object()) {
        for (tag, color) in colors {
            if let Some(color) = color.as_str() {
                story.tag_colors.insert(tag.clone(), color.to_string());
            }
        }
    }
    for (key, value) in &data {
        let expected = match key.as_str() {
            "ifid" | "format" | "format-version" | "start" => value.is_string(),
            "zoom" => value.is_number(),
            "tag-colors" => value.is_object(),
            other => {
                let known = ["ifid", "format", "format-version", "start", "tag-colors", "zoom"];
                let near = known.iter().find(|k| k.eq_ignore_ascii_case(other) || k.replace('-', "") == other.to_ascii_lowercase().replace(['-', '_'], ""));
                let hint = near.map(|k| format!("; did you mean \"{k}\"?")).unwrap_or_default();
                story.problems.push(problem(
                    "story-data",
                    false,
                    format!("StoryData has \"{other}\", which isn't one of its keys{hint}"),
                    file,
                    line,
                    Some("StoryData"),
                ));
                continue;
            }
        };
        if !expected {
            let should = match key.as_str() {
                "zoom" => "a number",
                "tag-colors" => "an object of tag names to colours",
                _ => "a string",
            };
            story.problems.push(problem(
                "story-data",
                false,
                format!("StoryData's \"{key}\" is {value}; it should be {should}"),
                file,
                line,
                Some("StoryData"),
            ));
        }
    }
    if let Some(colors) = data.get("tag-colors").and_then(|v| v.as_object()) {
        for (tag, color) in colors {
            if !color.as_str().is_some_and(|c| TAG_COLORS.contains(&c)) {
                let message = format!("The tag \"{tag}\" has the colour {color}; Twine knows only {}", TAG_COLORS.join(", "));
                story.problems.push(problem("story-data", false, message, file, line, Some("StoryData")));
            }
        }
    }
}

fn finish_twee(mut story: Story) -> Result<Story, Error> {
    let mut keep = vec![];
    for passage in std::mem::take(&mut story.passages) {
        match passage.name.as_str() {
            "StoryTitle" => {
                story.title = Some(passage.text.trim().to_string());
                story.title_passage = Some((passage.text.clone(), passage.file.clone(), passage.line));
            }
            "StoryData" => {
                story.data_at = Some((passage.file.clone(), passage.line));
                read_story_data(&mut story, &passage);
            }
            _ if passage.has_tag("script") => {
                story.scripts.push(Code { name: passage.name.clone(), file: passage.file.clone(), line: passage.body_line, text: passage.text })
            }
            _ if passage.has_tag("stylesheet") => {
                story.stylesheets.push(Code { name: passage.name.clone(), file: passage.file.clone(), line: passage.body_line, text: passage.text })
            }
            _ => keep.push(passage),
        }
    }
    story.passages = keep;
    merge_duplicates(&mut story);
    if story.start.is_none() && story.passage("Start").is_some() {
        story.start = Some("Start".into());
    }
    Ok(story)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_twee2_positions() {
        assert_eq!(from_twee2(":: Start [a b] <10,20>\nHi.\n:: Next <>\nThere."), ":: Start [a b] {\"position\":\"10,20\"}\nHi.\n:: Next\nThere.");
    }

    #[test]
    fn an_escaped_space_at_a_names_end_is_kept() {
        let passages = parse_twee(":: \\ Hall\\  [a]\nHi\n\n::   Plain   \nThere\n", Path::new("x.twee"));
        assert_eq!(passages[0].name, " Hall ");
        assert_eq!(passages[1].name, "Plain");
    }

    #[test]
    fn reads_twee_headers_with_escapes_tags_and_metadata() {
        let passages = parse_twee(":: A \\[b\\] c [one two] {\"position\":\"1,2\"}\nHello\n\n\n:: Next\nThere\n", Path::new("x.twee"));
        assert_eq!(passages.len(), 2);
        assert_eq!(passages[0].name, "A [b] c");
        assert_eq!(passages[0].tags, vec!["one", "two"]);
        assert_eq!(passages[0].text, "Hello");
        assert_eq!((passages[0].line, passages[0].body_line), (1, 2));
        assert_eq!(passages[1].line, 5);
    }
}
