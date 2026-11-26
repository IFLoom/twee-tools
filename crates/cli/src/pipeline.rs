//! One build, as `build` and `dev` both run it: settings from the command line over twee.toml, then read, check and
//! compile. It prints nothing; the caller shows what happened.

use std::path::{Path, PathBuf};

use check::config::Config;
use check::{Report, Severity, check_with};
use story::Story;

use crate::project::{Project, Public};

/// What to build and how, once twee.toml is taken into account.
#[derive(Debug, Clone)]
pub struct Settings {
    pub paths: Vec<PathBuf>,
    pub output: PathBuf,
    pub format: Option<String>,
    pub start: Option<String>,
    pub test: bool,
    pub head: Option<PathBuf>,
    pub modules: Vec<PathBuf>,
    pub embed_media: bool,
    pub archive: bool,
    pub json_output: bool,
    pub no_check: bool,
    pub strict: bool,
    pub exclude: Vec<String>,
    /// Check, and stop there: write nothing.
    pub check_only: bool,
    /// Keep each passage's text as written, not trimmed at both ends: `--no-trim`, or twee.toml's `trim = false`.
    pub no_trim: bool,
    /// Finding codes never reported: the command line's `--ignore`, then twee.toml's `[check] ignore`.
    pub ignore: Vec<String>,
    /// Severities for codes, from twee.toml's `[check] severity`.
    pub severity: std::collections::HashMap<String, Severity>,
    /// The folder whose contents are copied as they are beside the page: twee.toml's `public`, else `public` if it's there.
    pub public: Option<PathBuf>,
    /// Folders and files copied beside the page once it's built, each by its own name, from twee.toml's `assets`.
    pub assets: Vec<PathBuf>,
    /// Story format folders searched after every other, so theirs win: twee.toml's `format-paths`.
    pub format_paths: Vec<PathBuf>,
    /// Building for `twee-tools dev`: its `[dev]` settings apply.
    pub dev: bool,
    /// `dev`'s port and whether to open the browser: the command line's, else twee.toml's `[dev]`.
    pub port: Option<u16>,
    pub open: bool,
    /// More `*.twee-config.yml` files for the checker, besides those in the story's folder: the command line's, then
    /// twee.toml's `[check] config`.
    pub config_files: Vec<PathBuf>,
    /// What the command line says about a release copy; `new` combines it with twee.toml's `[compress]` into `compress`.
    pub compress_args: CompressArgs,
    /// A compressed copy to write after the build: where, and how.
    pub compress: Option<(PathBuf, media::Options)>,
}

/// The command line's release-copy options: each one set wins over twee.toml's.
#[derive(Debug, Clone, Default)]
pub struct CompressArgs {
    pub to: Option<PathBuf>,
    pub quality: Option<f32>,
    pub lossless: bool,
    pub effort: Option<u8>,
}

impl Settings {
    /// The command line's settings, with twee.toml (in the working folder) filling the gaps.
    pub fn new(mut args: Settings) -> Result<Settings, String> {
        let project = Project::find()?.unwrap_or_default();
        if args.paths.is_empty() {
            args.paths = project.sources.as_ref().map(|s| s.iter().map(|p| project.path(p)).collect()).unwrap_or_else(|| vec![project.path(Path::new("src"))]);
        }
        if args.output.as_os_str().is_empty() {
            args.output = project.output.as_ref().map(|p| project.path(p)).unwrap_or_else(|| project.path(Path::new("dist/index.html")));
        }
        args.format = args.format.or(project.format.clone());
        args.start = args.start.or(project.start.clone());
        args.head = args.head.or(project.head.as_ref().map(|p| project.path(p)));
        if args.modules.is_empty() {
            args.modules = project.modules.iter().flatten().map(|p| project.path(p)).collect();
        }
        args.embed_media |= project.embed_media.unwrap_or(false);
        args.strict |= project.strict.unwrap_or(false);
        args.no_trim |= project.trim == Some(false);
        // The command line's patterns (relative to the working folder) and twee.toml's (relative to it), together.
        args.exclude.extend(project.exclude.iter().flatten().map(|g| project.path(Path::new(g)).to_string_lossy().into_owned()));
        // A release copy when --compress-to asks for one or twee.toml turns it on: each setting from the command line,
        // else twee.toml, else the default.
        let table = project.compress.as_ref();
        if args.compress.is_none() && (args.compress_args.to.is_some() || table.and_then(|c| c.enabled).unwrap_or(false)) {
            let defaults = media::Options::default();
            let out = args
                .compress_args
                .to
                .clone()
                .or_else(|| table.and_then(|c| c.output.as_ref()).map(|p| project.path(p)))
                .unwrap_or_else(|| project.path(Path::new("release")));
            let options = media::Options {
                quality: args.compress_args.quality.or(table.and_then(|c| c.quality)).unwrap_or(defaults.quality),
                lossless: args.compress_args.lossless || table.and_then(|c| c.lossless).unwrap_or(false),
                effort: args.compress_args.effort.or(table.and_then(|c| c.effort)).unwrap_or(defaults.effort),
                max_width: table.and_then(|c| c.max_width),
                max_height: table.and_then(|c| c.max_height),
                exclude: table.and_then(|c| c.exclude.clone()).unwrap_or_default(),
                ..defaults
            };
            args.compress = Some((out, options));
        }
        args.take_check(&project)?;
        args.assets.extend(project.assets.iter().flatten().map(|p| project.path(p)));
        args.public = match &project.public {
            None | Some(Public::On(true)) => Some(project.path(Path::new("public"))).filter(|p| p.is_dir()),
            Some(Public::On(false)) => None,
            Some(Public::Folder(folder)) => {
                let folder = project.path(folder);
                if !folder.is_dir() {
                    return Err(format!("twee.toml: public is {}, and there's no such folder", folder.display()));
                }
                Some(folder)
            }
        };
        args.format_paths.extend(project.format_paths.iter().flatten().map(|p| project.path(p)));
        if args.dev
            && let Some(dev) = &project.dev
        {
            args.port = args.port.or(dev.port);
            args.open |= dev.open.unwrap_or(false);
            args.test |= dev.test.unwrap_or(false);
        }
        Ok(args)
    }

    /// Empty settings, to gather a project's [check] into for `check`, which builds nothing.
    pub fn empty() -> Settings {
        Settings {
            paths: vec![],
            output: PathBuf::new(),
            format: None,
            start: None,
            test: false,
            head: None,
            modules: vec![],
            embed_media: false,
            archive: false,
            json_output: false,
            no_check: false,
            strict: false,
            exclude: vec![],
            check_only: false,
            no_trim: false,
            config_files: vec![],
            ignore: vec![],
            severity: Default::default(),
            public: None,
            assets: vec![],
            format_paths: vec![],
            dev: false,
            port: None,
            open: false,
            compress_args: CompressArgs::default(),
            compress: None,
        }
    }

    /// twee.toml's [check]: more checker settings files, codes to ignore, and severities.
    pub fn take_check(&mut self, project: &Project) -> Result<(), String> {
        self.config_files.extend(project.check.iter().flat_map(|c| c.config.iter().flatten()).map(|p| project.path(p)));
        self.ignore.extend(project.check.iter().flat_map(|c| c.ignore.iter().flatten()).cloned());
        for (code, level) in project.check.iter().flat_map(|c| c.severity.iter().flatten()) {
            let severity = match level.to_ascii_lowercase().as_str() {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                "note" => Severity::Note,
                other => return Err(format!("twee.toml: [check] severity gives {code} \"{other}\"; it can be error, warning or note")),
            };
            self.severity.insert(code.clone(), severity);
        }
        Ok(())
    }

    /// The files a change to which needs a rebuild: the sources, the head file and the modules.
    pub fn watched(&self) -> Vec<PathBuf> {
        let mut out = self.paths.clone();
        out.extend(self.head.clone());
        out.extend(self.modules.clone());
        out.extend(self.assets.clone());
        out.extend(self.public.clone());
        out
    }

    /// Everything copied beside the page, each by its own name: what's in `public` (looked at afresh, so a file added
    /// while `dev` runs is seen), then `assets`.
    pub fn copied(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = vec![];
        if let Some(public) = &self.public
            && let Ok(entries) = std::fs::read_dir(public)
        {
            let mut inside: Vec<PathBuf> =
                entries.flatten().map(|e| e.path()).filter(|p| !p.file_name().and_then(|n| n.to_str()).is_some_and(story::passed_by)).collect();
            inside.sort();
            out.extend(inside);
        }
        out.extend(self.assets.iter().cloned());
        out
    }
}

/// How a build ended.
pub enum Outcome {
    /// Built: the text written (a page, an archive or JSON), and the checker's report if it had anything to say.
    Built { text: String, story: Box<Story>, report: Option<Report>, passages: usize, format: String, notes: Vec<String> },
    /// The checker found errors (or warnings, with strict), so nothing was built.
    Stopped { story: Box<Story>, report: Report },
    /// Checked only, as asked: the whole report, notes included. Nothing was built.
    Checked { story: Box<Story>, report: Report },
    /// Something else stopped it: a story that can't be read, no such format, no IFID.
    Failed(String),
}

/// One build.
pub fn run(settings: &Settings, progress: check::Progress) -> Outcome {
    progress(&format!("Reading {}", settings.paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")));
    let mut story = match Story::read_all_options(&settings.paths, &settings.exclude, !settings.no_trim) {
        Ok(story) => story,
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    let mut config = match Config::find(&story.root) {
        Ok(config) => config,
        Err(error) => return Outcome::Failed(error),
    };
    for file in &settings.config_files {
        if let Err(error) = config.add(file) {
            return Outcome::Failed(error);
        }
    }
    config.ignore.extend(settings.ignore.iter().cloned());
    config.severity.extend(settings.severity.iter().map(|(code, s)| (code.clone(), *s)));
    if story.form == story::Form::Twee {
        build::sources::add_assets_except(&mut story, &settings.paths, settings.embed_media, &settings.exclude);
        // A page's media paths are relative to where the page will be: check them there.
        if let Some(dir) = settings.output.parent().filter(|d| !d.as_os_str().is_empty() && settings.output.as_os_str() != "-") {
            story.media_root = dir.to_path_buf();
        }
        // The assets will be beside the page once it's built: check the media in them as if they were already there.
        story.media_mounts = settings.copied().iter().filter_map(|a| Some((a.clone(), a.file_name()?.to_string_lossy().into_owned()))).collect();
    }
    progress("Finding the story format");
    let (formats, mut notes) = build::formats::all(&format_folders(settings));
    let format = match build::formats::choose(&formats, settings.format.as_deref(), story.format.as_deref(), story.format_version.as_deref()) {
        Ok(format) => format,
        Err(error) => return Outcome::Failed(error),
    };
    let mut kept_report = None;
    if settings.check_only {
        let report = check_with(&story, &config, progress);
        return Outcome::Checked { story: Box::new(story), report };
    }
    if !settings.no_check {
        let mut report = check_with(&story, &config, progress);
        report.findings.retain(|f| f.severity != Severity::Note);
        if fails(&report, settings.strict) {
            return Outcome::Stopped { story: Box::new(story), report };
        }
        if !report.findings.is_empty() {
            kept_report = Some(report);
        }
    }
    progress("Building");
    let mut head = match build::sources::modules(&settings.modules) {
        Ok(modules) => modules,
        Err(error) => return Outcome::Failed(error),
    };
    if let Some(file) = &settings.head {
        match std::fs::read_to_string(file) {
            Ok(text) => head.push(text.trim().to_string()),
            Err(e) => return Outcome::Failed(format!("{}: {e}", file.display())),
        }
    }
    let options = build::output::Options { start: settings.start.clone(), test: settings.test, head };
    let label = format!("{} {}", format.name, format.version);
    let (text, passages) = if settings.archive {
        match build::output::archive(&story, format, &options) {
            Ok(text) => (text, story.passages.len()),
            Err(error) => return Outcome::Failed(error),
        }
    } else if settings.json_output {
        match build::output::json(&story, format, &options) {
            Ok(text) => (text, story.passages.len()),
            Err(error) => return Outcome::Failed(error),
        }
    } else {
        match build::output::compile(&story, format, &options) {
            Ok(page) => {
                notes.extend(page.notes);
                (page.html, page.passages)
            }
            Err(error) => return Outcome::Failed(error),
        }
    };
    let origin = match &format.origin {
        build::formats::Origin::BuiltIn => "built in".to_string(),
        build::formats::Origin::Folder(dir) => dir.display().to_string(),
    };
    Outcome::Built { text, story: Box::new(story), report: kept_report, passages, format: format!("{label} ({origin})"), notes }
}

/// Where story formats are looked for: where Tweego looks, then the project's own folders, which win.
pub fn format_folders(settings: &Settings) -> Vec<PathBuf> {
    let mut folders = build::formats::search_folders();
    folders.extend(settings.format_paths.iter().filter(|p| p.is_dir()).cloned());
    folders
}

/// Whether a report stops a build: any error, or with strict, any warning. `check` and `build --check-only` fail by the
/// same rule, so each says no exactly when a build would.
pub fn fails(report: &Report, strict: bool) -> bool {
    report.errors() > 0 || (strict && report.findings.iter().any(|f| f.severity == Severity::Warning))
}

/// Copies the assets beside the page: each folder or file by its own name (`src/media` to `dist/media`), past folders no
/// copy takes (`.git`, `node_modules`). A file already there with the same size and time isn't copied again, so a rebuild
/// in `dev` is quick. Gives how many files it copied.
pub fn copy_assets(assets: &[PathBuf], output: &Path) -> Result<usize, String> {
    let Some(dir) = output.parent().filter(|_| output.as_os_str() != "-") else { return Ok(0) };
    let mut copied = 0;
    for asset in assets {
        let Some(name) = asset.file_name() else { continue };
        let target = dir.join(name);
        // A file of the page's own name would replace the page just built.
        if target == output {
            eprintln!("note: {} has the built page's name, so it isn't copied over it", asset.display());
            continue;
        }
        copied += copy_tree(asset, &target)?;
    }
    Ok(copied)
}

fn copy_tree(from: &Path, to: &Path) -> Result<usize, String> {
    if from.is_file() {
        let same = |a: &std::fs::Metadata, b: &std::fs::Metadata| a.len() == b.len() && a.modified().ok() == b.modified().ok();
        if let (Ok(a), Ok(b)) = (from.metadata(), to.metadata())
            && same(&a, &b)
        {
            return Ok(0);
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::copy(from, to).map_err(|e| format!("{}: {e}", from.display()))?;
        // Keep the time, so the next build knows it's the same.
        if let Ok(time) = from.metadata().and_then(|m| m.modified()) {
            let _ = std::fs::File::options().write(true).open(to).and_then(|f| f.set_modified(time));
        }
        return Ok(1);
    }
    let entries = std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut copied = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()).is_some_and(story::passed_by) {
            continue;
        }
        copied += copy_tree(&path, &to.join(entry.file_name()))?;
    }
    Ok(copied)
}

/// Writes a build's text to its output, making the folder.
pub fn write(output: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(output, text).map_err(|e| format!("{}: {e}", output.display()))
}
