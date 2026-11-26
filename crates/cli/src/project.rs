//! `twee.toml`: a project's settings for `build` and `dev`, so neither needs a list of options. Options on the command line
//! win over it. Paths in it are relative to the folder it's in.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Project {
    /// Twee files and folders of them. Default: `src`.
    pub sources: Option<Vec<PathBuf>>,
    /// The page to write. Default: `dist/index.html`.
    pub output: Option<PathBuf>,
    /// A story format, `SugarCube` or `SugarCube@2.37.3`; default: the story's own.
    pub format: Option<String>,
    pub start: Option<String>,
    pub head: Option<PathBuf>,
    pub modules: Option<Vec<PathBuf>>,
    /// Files to leave out, as globs relative to this file: `["src/drafts/**"]`.
    pub exclude: Option<Vec<String>>,
    pub embed_media: Option<bool>,
    /// Stop on warnings, not only errors.
    pub strict: Option<bool>,
    pub compress: Option<Compress>,
    pub check: Option<CheckSettings>,
    pub dev: Option<DevSettings>,
    /// The folder whose contents are copied, as they're laid out, beside the built page: `public/media/sea.png` becomes
    /// `dist/media/sea.png`. `"static"` names another folder, `false` turns it off; by default, `public` if it's there.
    pub public: Option<Public>,
    /// More folders (or files) copied beside the built page, each by its own name: `["../shared/art"]` gives `dist/art`.
    pub assets: Option<Vec<PathBuf>>,
    /// Folders of story formats to look in before any other (each holding `<id>/format.js`), so a project can carry its
    /// own: they win over the built-in formats and those installed where Tweego looks.
    pub format_paths: Option<Vec<PathBuf>>,
    /// The oldest twee-tools that builds this project, `"0.4"`: an older one says to upgrade and stops.
    pub requires: Option<String>,
    /// Trim spaces and blank lines from both ends of each passage, as Tweego does (the default); `false` keeps them.
    pub trim: Option<bool>,
    /// Where it was read from.
    #[serde(skip)]
    pub root: PathBuf,
}

/// The checker's settings: its own come from `*.twee-config.yml` files (the Twee 3 Language Tools format) in the
/// story's folder; these name more.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct CheckSettings {
    /// More `*.twee-config.yml` files, relative to twee.toml: `["macros.twee-config.yml"]`.
    pub config: Option<Vec<PathBuf>>,
    /// Finding codes never reported: `["unused-media", "unwritten"]`.
    pub ignore: Option<Vec<String>>,
    /// A severity for a code, over the checker's own: `{ dead-end = "error" }`.
    pub severity: Option<std::collections::BTreeMap<String, String>>,
}

/// `public = "static"` or `public = false`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Public {
    On(bool),
    Folder(PathBuf),
}

/// `twee-tools dev`'s own settings.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DevSettings {
    pub port: Option<u16>,
    /// Open the browser when it starts.
    pub open: Option<bool>,
    /// Build in test mode (SugarCube's debug views) while developing; `build` never does unless asked.
    pub test: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Compress {
    /// On every build, also write a compressed copy (the page and its media, re-encoded) to `output`.
    pub enabled: Option<bool>,
    /// Where the compressed copy goes. Default: `release`.
    pub output: Option<PathBuf>,
    /// 0 to 100: lossy, how close to the original; lossless, how hard to work on the size. Default 80.
    pub quality: Option<f32>,
    /// Encode losslessly, every pixel kept exactly. Default false.
    pub lossless: Option<bool>,
    /// How hard to work on each image, 0 (fast) to 6 (smallest). Default 4.
    pub effort: Option<u8>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    /// Files beside the page to leave out of the compressed copy, as globs relative to the page's folder:
    /// `["**/*.psd", "notes/**"]`.
    pub exclude: Option<Vec<String>>,
}

impl Project {
    /// The nearest `twee.toml`: in the working folder, or the first folder above it that has one, as Cargo finds
    /// Cargo.toml. Its paths stay relative to where it is (`../..` from a folder two below).
    pub fn find() -> Result<Option<Project>, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        for (up, dir) in cwd.ancestors().enumerate() {
            if dir.join("twee.toml").is_file() {
                let relative: PathBuf = std::iter::repeat_n("..", up).collect();
                return Project::read(&relative.join("twee.toml")).map(Some);
            }
        }
        Ok(None)
    }

    pub fn read(file: &Path) -> Result<Project, String> {
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut project: Project = toml::from_str(&text).map_err(|e| {
            // A key this twee-tools doesn't know is most often one a newer twee-tools added.
            let newer = if e.message().starts_with("unknown field") {
                format!("\nIf that's from a newer twee-tools, upgrade (this is {}): `twee-tools upgrade`.", env!("CARGO_PKG_VERSION"))
            } else {
                String::new()
            };
            format!("{}: {e}{newer}", file.display())
        })?;
        project.root = file.parent().map(Path::to_path_buf).unwrap_or_default();
        if let Some(required) = &project.requires {
            let have = env!("CARGO_PKG_VERSION");
            if !at_least(have, required) {
                return Err(format!(
                    "{} needs twee-tools {required} or later, and this is {have}: upgrade it (see https://github.com/IFLoom/twee-tools).",
                    file.display()
                ));
            }
        }
        Ok(project)
    }

    /// A path from the file, relative to where the file is.
    pub fn path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() || self.root.as_os_str().is_empty() { path.to_path_buf() } else { self.root.join(path) }
    }
}

/// Whether a version (`0.4.1`) is at least another (`0.4`, `0.4.0`): each part compared as a number, missing ones 0.
pub fn at_least(have: &str, wanted: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.trim().split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect() };
    let (mut have, mut wanted) = (parts(have), parts(wanted));
    have.resize(3, 0);
    wanted.resize(3, 0);
    have >= wanted
}

/// The files `init` makes, for a story format.
pub fn starter(name: &str, format: &str, version: &str, ifid: &str) -> Vec<(&'static str, String)> {
    let start = match format {
        "Harlowe" => "Welcome to (print: \"your story\").\n\n[[Begin|The End]]",
        "Chapbook" => "Welcome to your story.\n\n[[Begin->The End]]",
        "Snowman" => "Welcome to your story.\n\n[[Begin->The End]]",
        _ => "Welcome to <<print \"your story\">>.\n\n[[Begin|The End]]",
    };
    let tool = env!("CARGO_PKG_VERSION");
    vec![
        (
            "twee.toml",
            format!(
                "# twee-tools project: `twee-tools build` and `twee-tools dev` read this.\nsources = [\"src\"]\noutput = \"dist/index.html\"\nformat = \"{format}@{version}\"    # pinned, so a newer twee-tools builds the same page\n# public = \"public\"               # copied as it is beside the page (public/media/sea.png: dist/media/sea.png); the default when it's there\n# assets = [\"../shared/art\"]      # more folders copied beside the page, each by its name: dist/art\n# format-paths = [\"formats\"]      # the project's own story formats, before any other\n# requires = \"{tool}\"             # the oldest twee-tools that builds this project\n# exclude = [\"src/drafts/**\", \"**/*.bak\"]    # sources to leave out of the build\n# strict = true\n# trim = false     # keep spaces and blank lines at the ends of passages (Tweego's --no-trim)\n\n# [check]\n# config = [\"macros.twee-config.yml\"]   # more checker settings: macros and variables the story's code adds\n# ignore = [\"unused-media\"]              # finding codes never reported\n# severity = {{ dead-end = \"error\" }}    # a code's severity: error, warning or note\n\n# [dev]\n# port = 8080\n# open = true\n# test = true       # SugarCube's debug views while developing\n\n# [compress]\n# enabled = true    # also write a release: the page and its media, re-encoded smaller\n# output = \"release\"\n# quality = 80      # 0 to 100\n# lossless = false  # true keeps every pixel exactly\n# effort = 4        # 0 (fast) to 6 (smallest)\n# exclude = [\"**/*.psd\", \"notes/**\"]    # files beside the page to leave out of the release\n"
            ),
        ),
        (
            "src/story.twee",
            format!(
                ":: StoryTitle\n{name}\n\n:: StoryData\n{{\n  \"ifid\": \"{ifid}\",\n  \"format\": \"{format}\",\n  \"format-version\": \"{version}\",\n  \"start\": \"Start\"\n}}\n\n:: Start\n{start}\n\n:: The End\nThe story ends here, for now: write more passages, and link to them.\n"
            ),
        ),
        ("src/story.js", "// The story's JavaScript.\n".into()),
        ("src/story.css", "/* The story's stylesheet. */\n".into()),
        ("public/.gitkeep", "".into()),
        (".gitignore", "dist/\n".into()),
    ]
}
