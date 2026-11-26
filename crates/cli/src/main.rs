//! `twee-tools build`, `check`, `compress` and `formats`.

mod dev;
mod pipeline;
mod project;
mod render;
mod spinner;

use std::path::PathBuf;
use std::process::ExitCode;

use check::config::Config;
use check::{Severity, check_with};

use clap::{Parser, Subcommand, ValueEnum};
use story::Story;

#[derive(Parser)]
#[command(version, about = "Builds, checks and shrinks Twine stories written in Twee.")]
struct Cli {
    /// Run as if started in this folder: twee.toml and every path are found from there.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    directory: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a story: Twee (and its JavaScript, CSS and media) with a story format, to a page to play. It's checked first,
    /// and errors stop the build.
    Build {
        /// Twee files and folders of them (read with everything below), as one story. Default: twee.toml's sources,
        /// else `src`.
        paths: Vec<PathBuf>,
        /// Where to write the page; `-` for standard output. Default: twee.toml's output, else `dist/index.html`.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// The story format: a name, a name@version, or an ID (`sugarcube`, `SugarCube@2.37.3`, `harlowe-3`). Default:
        /// the story's own, else SugarCube.
        #[arg(short, long)]
        format: Option<String>,
        /// The passage to start at, over the story's own.
        #[arg(short, long)]
        start: Option<String>,
        /// Test mode: SugarCube's debug views.
        #[arg(short, long)]
        test: bool,
        /// A file whose HTML goes into the page's <head>.
        #[arg(long)]
        head: Option<PathBuf>,
        /// JavaScript, CSS and font files (or folders of them) to put in the page's <head>, before the story runs.
        #[arg(short, long)]
        module: Vec<PathBuf>,
        /// Put the images, audio and video under the source paths into the page, as Twine media passages, as Tweego does.
        /// Without it they stay files beside the page.
        #[arg(long)]
        embed_media: bool,
        /// Leave out source files that match a glob (repeatable): `--exclude "src/drafts/**"`, `--exclude "**/*.bak"`.
        /// Relative to the working folder; twee.toml's `exclude` applies too.
        #[arg(long)]
        exclude: Vec<String>,
        /// Write a Twine archive (the story's data alone, for Twine's library) rather than a page.
        #[arg(long, conflicts_with = "json_output")]
        archive: bool,
        /// Write the story as Twine 2's JSON rather than a page.
        #[arg(long)]
        json_output: bool,
        /// Also write a copy for release to this folder: the page, and the media it uses (found beside the page)
        /// re-encoded smaller. The page you built is left as it is.
        #[arg(long)]
        compress_to: Option<PathBuf>,
        /// WebP quality for the release copy, 0 to 100 (default: twee.toml's, else 80).
        #[arg(long)]
        quality: Option<f32>,
        /// Encode the release copy's images losslessly: every pixel kept exactly.
        #[arg(long)]
        lossless: bool,
        /// How hard to work on each image of the release copy, 0 (fast) to 6 (smallest) (default: twee.toml's, else 4).
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=6))]
        effort: Option<u8>,
        /// Build without checking.
        #[arg(long)]
        no_check: bool,
        /// Check the sources exactly as a build would, show the whole report, and write nothing. Fails (exit 1) when a
        /// build would stop: for CI, `twee-tools build --check-only --strict`.
        #[arg(long, conflicts_with_all = ["no_check", "archive", "json_output", "compress_to"])]
        check_only: bool,
        /// Stop on warnings too, not only errors.
        #[arg(long)]
        strict: bool,
        /// More checker settings: a twee-config.yml with the story's custom macros (any in the story's folder are read too).
        #[arg(long)]
        config: Vec<PathBuf>,
        /// A finding code never to report (repeatable): `--ignore unused-media`. twee.toml's [check] ignore applies too.
        #[arg(long)]
        ignore: Vec<String>,
        /// With --check-only: print the report as JSON.
        #[arg(long, requires = "check_only")]
        json: bool,
        /// With --check-only: leave warnings out of the report.
        #[arg(long, requires = "check_only")]
        errors_only: bool,
        /// With --check-only: show every entry of a long list of warnings.
        #[arg(long, requires = "check_only")]
        all: bool,
        /// Colour the report: auto, always or never.
        #[arg(long, value_enum, default_value_t = Color::Auto)]
        color: Color,
    },
    /// Build, serve and watch: every save builds the story again and reloads the browser; errors show over the last good
    /// build, which keeps being served.
    Dev {
        /// Twee files and folders of them. Default: twee.toml's sources, else `src`.
        paths: Vec<PathBuf>,
        /// Where to write the page as well. Default: twee.toml's output, else `dist/index.html`.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// The port to serve on (the next free one up, if it's taken). Default: twee.toml's [dev] port, else 8080.
        #[arg(short, long)]
        port: Option<u16>,
        /// Open the browser.
        #[arg(long)]
        open: bool,
        /// The story format, over the story's own.
        #[arg(short, long)]
        format: Option<String>,
        /// Test mode: SugarCube's debug views.
        #[arg(short, long)]
        test: bool,
        /// Build without checking.
        #[arg(long)]
        no_check: bool,
        /// Leave out source files that match a glob (repeatable); twee.toml's `exclude` applies too.
        #[arg(long)]
        exclude: Vec<String>,
        /// More checker settings: a twee-config.yml with the story's custom macros.
        #[arg(long)]
        config: Vec<PathBuf>,
        /// A finding code never to report (repeatable): `--ignore unused-media`. twee.toml's [check] ignore applies too.
        #[arg(long)]
        ignore: Vec<String>,
        /// Colour the report: auto, always or never.
        #[arg(long, value_enum, default_value_t = Color::Auto)]
        color: Color,
    },
    /// Start a project: twee.toml, and a story with a fresh IFID, its JavaScript and stylesheet.
    Init {
        /// The folder to make it in (the working folder if left out).
        dir: Option<PathBuf>,
        /// The story format: sugarcube, harlowe, chapbook or snowman.
        #[arg(short, long, default_value = "sugarcube")]
        format: String,
        /// The story's title (default: the folder's name).
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Turn a published story (or a Twine archive) back into Twee, to keep in files and git.
    Decompile {
        /// The published .html story.
        story: PathBuf,
        /// Where to write the Twee; standard output if left out.
        #[arg(short, long, conflicts_with = "split")]
        output: Option<PathBuf>,
        /// Write a folder: story.twee, story.js and story.css.
        #[arg(long)]
        split: Option<PathBuf>,
    },
    /// The story formats: the built-in ones, and any installed where Tweego looks.
    Formats {
        #[command(subcommand)]
        what: Option<FormatsCommand>,
    },
    /// Check a published page (.html): links, passages nothing reaches, variables, macros and media. Twee: build --check-only.
    Check {
        /// A published story: the .html page Twine (or `twee-tools build`) wrote, or a folder holding one. Twee sources are
        /// checked by `twee-tools build --check-only`, which reads them as the build does.
        #[arg(required = true, num_args = 1..)]
        story: Vec<PathBuf>,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
        /// A twee-config.yml with the story's custom macros (any in the story's folder are read too).
        #[arg(long)]
        config: Vec<PathBuf>,
        /// A finding code never to report (repeatable): `--ignore unused-media`. twee.toml's [check] ignore applies too.
        #[arg(long)]
        ignore: Vec<String>,
        /// Leave warnings out.
        #[arg(long)]
        errors_only: bool,
        /// Show every entry of a long list of warnings.
        #[arg(long)]
        all: bool,
        /// Fail on warnings too, not only errors.
        #[arg(long)]
        strict: bool,
        /// Colour the report: auto (when it's a terminal, and NO_COLOR isn't set), always or never.
        #[arg(long, value_enum, default_value_t = Color::Auto)]
        color: Color,
    },
    /// Write a copy of a story with its images re-encoded smaller, and the story pointing at them.
    Compress {
        story: PathBuf,
        /// The folder to write the copy to. It must not be the story's own folder.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Report what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// WebP quality, 0 to 100. Lossy: how close to the original; lossless: how hard to work on the size.
        #[arg(long, default_value_t = 80.0)]
        quality: f32,
        /// Encode losslessly: every pixel kept exactly. Often smaller than PNG for flat art; bigger than lossy for photos.
        #[arg(long)]
        lossless: bool,
        /// How hard to work on each image, 0 (fast) to 6 (smallest).
        #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u8).range(0..=6))]
        effort: u8,
        /// Scale images down to fit this width.
        #[arg(long)]
        max_width: Option<u32>,
        /// Scale images down to fit this height.
        #[arg(long)]
        max_height: Option<u32>,
        /// Also convert files the story can't be rewritten for, keeping their names (browsers read an image by its content).
        #[arg(long)]
        keep_names: bool,
        /// Leave media nothing uses out of the copy.
        #[arg(long)]
        drop_unused: bool,
        /// Leave files out of the copy that match a glob, relative to the story's folder (repeatable):
        /// `--exclude "**/*.psd"`. The copy is refused if the story uses one.
        #[arg(long)]
        exclude: Vec<String>,
        /// What to encode to.
        #[arg(long, value_enum, default_value_t = Format::Webp)]
        format: Format,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum FormatsCommand {
    /// The formats there are (the default).
    List,
    /// The folders searched for installed formats.
    Path,
    /// The licences of the formats built into this program.
    Licenses,
}

#[derive(Clone, Copy, ValueEnum)]
pub(crate) enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Webp,
}

fn main() -> ExitCode {
    // Piped into something that stops reading (`twee-tools formats licenses | head`), end quietly as other tools do,
    // rather than panic on the closed pipe: Rust ignores SIGPIPE unless told otherwise.
    #[cfg(unix)]
    // SAFETY: setting a signal's handler to the default, before any other thread starts.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    if let Some(dir) = &cli.directory
        && let Err(e) = std::env::set_current_dir(dir)
    {
        eprintln!("{}: {e}", dir.display());
        return ExitCode::from(2);
    }
    match cli.command {
        Command::Build {
            paths,
            output,
            format,
            start,
            test,
            head,
            module,
            embed_media,
            exclude,
            archive,
            json_output,
            compress_to,
            quality,
            lossless,
            effort,
            no_check,
            check_only,
            strict,
            config,
            ignore,
            json,
            errors_only,
            all,
            color,
        } => {
            let settings = pipeline::Settings {
                paths,
                output: output.unwrap_or_default(),
                format,
                start,
                test,
                head,
                modules: module,
                embed_media,
                archive,
                json_output,
                no_check,
                strict,
                exclude,
                check_only,
                config_files: config,
                ignore,
                severity: Default::default(),
                public: None,
                assets: vec![],
                format_paths: vec![],
                dev: false,
                port: None,
                open: false,
                compress_args: pipeline::CompressArgs { to: compress_to, quality, lossless, effort },
                compress: None,
            };
            run_build(settings, ReportOptions { json, errors_only, all, color })
        }
        Command::Decompile { story, output, split } => run_decompile(story, output, split),
        Command::Init { dir, format, name } => run_init(dir, &format, name),
        Command::Dev { paths, output, port, open, format, test, no_check, exclude, config, ignore, color } => {
            let settings = pipeline::Settings {
                paths,
                output: output.unwrap_or_default(),
                format,
                start: None,
                test,
                head: None,
                modules: vec![],
                embed_media: false,
                archive: false,
                json_output: false,
                no_check,
                strict: false,
                exclude,
                check_only: false,
                config_files: config,
                ignore,
                severity: Default::default(),
                public: None,
                assets: vec![],
                format_paths: vec![],
                dev: true,
                port,
                open,
                compress_args: pipeline::CompressArgs::default(),
                compress: None,
            };
            dev::run(settings, color)
        }
        Command::Formats { what } => run_formats(what.unwrap_or(FormatsCommand::List)),
        Command::Check { story, json, config, ignore, errors_only, all, strict, color } => {
            run_check(story, config, ignore, strict, ReportOptions { json, errors_only, all, color })
        }
        Command::Compress { story, out, dry_run, quality, lossless, effort, max_width, max_height, keep_names, drop_unused, exclude, format: _, json } => {
            let options = media::Options { quality, lossless, effort, max_width, max_height, keep_names, drop_unused, dry_run, exclude };
            run_compress(story, out, options, json)
        }
    }
}

fn load(paths: &[PathBuf], extra: &[PathBuf]) -> Result<(Story, Config), String> {
    let story = Story::read_all(paths).map_err(|e| e.to_string())?;
    let mut config = Config::find(&story.root)?;
    for file in extra {
        config.add(file)?;
    }
    Ok((story, config))
}

/// How to show a whole report: `check`'s, and `build --check-only`'s.
struct ReportOptions {
    json: bool,
    errors_only: bool,
    all: bool,
    color: Color,
}

/// Shows a whole report on standard output, as JSON or for reading. `errors_only` filters what's shown, never whether
/// it fails.
fn print_report(story: &Story, mut report: check::Report, options: &ReportOptions) {
    use std::io::IsTerminal;
    if options.errors_only {
        report.findings.retain(|f| f.severity == Severity::Error);
    }
    if options.json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    } else {
        let width = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(100);
        let style = render::Style::new(color_on(options.color, std::io::stdout().is_terminal()));
        print!("{}", render::report(story, &report, &style, &render::Options { all: options.all, width }));
    }
}

fn run_check(paths: Vec<PathBuf>, mut extra: Vec<PathBuf>, mut ignore: Vec<String>, strict: bool, options: ReportOptions) -> ExitCode {
    // A project's [check] settings apply to the pages it makes: `check dist/index.html` reads the same ones as the build.
    let project = match project::Project::find() {
        Ok(project) => project,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let mut severity = std::collections::HashMap::new();
    if let Some(project) = &project {
        let mut settings = pipeline::Settings::empty();
        match settings.take_check(project) {
            Ok(()) => {
                extra.extend(settings.config_files);
                ignore.extend(settings.ignore);
                severity = settings.severity;
            }
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    }
    let mut spinner = spinner::Spinner::start(&format!("Reading {}", paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")));
    let (story, config) = match load(&paths, &extra) {
        Ok(loaded) => loaded,
        Err(error) => {
            spinner.finish();
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    if story.form == story::Form::Twee {
        spinner.finish();
        let what = paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
        eprintln!(
            "{what} is Twee sources, and `check` reads a published page. Check the sources with `twee-tools build --check-only` \
             (it reads them as the build does), or check the page a build writes: `twee-tools check dist/index.html`."
        );
        return ExitCode::from(2);
    }
    let mut config = config;
    config.ignore.extend(ignore);
    config.severity.extend(severity);
    let report = check_with(&story, &config, &|stage| spinner.set(stage));
    spinner.finish();
    let failed = pipeline::fails(&report, strict);
    print_report(&story, report, &options);
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn color_on(color: Color, stream_is_terminal: bool) -> bool {
    match color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => stream_is_terminal && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    }
}

/// Shows a checker report on standard error.
pub(crate) fn show_report(story: &Story, report: &check::Report, color: Color) {
    use std::io::IsTerminal;
    let style = render::Style::new(color_on(color, std::io::stderr().is_terminal()));
    let width = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(100);
    eprint!("{}", render::report(story, report, &style, &render::Options { all: false, width }));
}

fn run_build(settings: pipeline::Settings, report_options: ReportOptions) -> ExitCode {
    let color = report_options.color;
    use std::io::IsTerminal;
    let started = std::time::Instant::now();
    let settings = match pipeline::Settings::new(settings) {
        Ok(s) => s,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let mut spinner = spinner::Spinner::start("Reading the story");
    let outcome = pipeline::run(&settings, &|stage| spinner.set(stage));
    spinner.finish();
    match outcome {
        pipeline::Outcome::Failed(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
        pipeline::Outcome::Checked { story, report } => {
            let failed = pipeline::fails(&report, settings.strict);
            print_report(&story, report, &report_options);
            if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
        }
        pipeline::Outcome::Stopped { story, report } => {
            show_report(&story, &report, color);
            let why = if report.errors() > 0 { "errors" } else { "warnings, with strict on" };
            eprintln!(" Not built: the story has {why}. Fix them, or build anyway with --no-check.\n");
            ExitCode::from(1)
        }
        pipeline::Outcome::Built { text, story, report, passages, format, notes } => {
            if let Some(report) = &report {
                show_report(&story, report, color);
            }
            for note in &notes {
                eprintln!("note: {note}");
            }
            if settings.output.as_os_str() == "-" {
                print!("{text}");
                return ExitCode::SUCCESS;
            }
            if let Err(error) = pipeline::write(&settings.output, &text) {
                eprintln!("{error}");
                return ExitCode::from(1);
            }
            let mut extra = String::new();
            match pipeline::copy_assets(&settings.copied(), &settings.output) {
                Ok(0) => {}
                Ok(n) => extra = format!("; {n} asset files copied"),
                Err(error) => {
                    eprintln!("{error}");
                    return ExitCode::from(1);
                }
            }
            if let Some((dir, options)) = &settings.compress {
                match compress_copy(&settings.output, dir, options) {
                    Ok(summary) => extra.push_str(&format!("; {summary}")),
                    Err(error) => {
                        eprintln!("{error}");
                        return ExitCode::from(1);
                    }
                }
            }
            let style = render::Style::new(color_on(color, std::io::stderr().is_terminal()));
            let what = if settings.archive {
                "archive"
            } else if settings.json_output {
                "JSON"
            } else {
                "page"
            };
            eprintln!(
                "{} Built {} ({what}): {passages} passages, {format}, {} in {:.1}s{extra}",
                style.green_mark(),
                settings.output.display(),
                check::megabytes(text.len() as u64),
                started.elapsed().as_secs_f64()
            );
            ExitCode::SUCCESS
        }
    }
}

/// A release copy of the built page: the page and its media (found beside it) into `dir`, the media re-encoded smaller
/// and the page pointing at them. The built page and its media are left as they are. Says what it saved.
fn compress_copy(page: &std::path::Path, dir: &std::path::Path, options: &media::Options) -> Result<String, String> {
    // Only a folder this made before is replaced: it holds the marker. Anything else that isn't empty is refused, so
    // `--compress-to .` can never clear a project.
    const MARKER: &str = ".twee-tools-release";
    let has_files = std::fs::read_dir(dir).map(|mut d| d.next().is_some()).unwrap_or(false);
    if has_files {
        if !dir.join(MARKER).is_file() {
            return Err(format!("{} isn't empty, and twee-tools didn't make it, so it won't write a release there; give a new folder", dir.display()));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let story = Story::read(page).map_err(|e| e.to_string())?;
    let report = media::compress(&story, dir, options)?;
    std::fs::write(dir.join(MARKER), "A release twee-tools made; it replaces this folder on the next build.\n")
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let saved = report.before.saturating_sub(report.after);
    Ok(format!("a compressed copy in {}, media {} → {}", dir.display(), check::megabytes(report.before), check::megabytes(report.before - saved)))
}

/// Writes a build's text to a file (or standard output for `-`) and says what was built.
fn write_output(output: &std::path::Path, text: &str, color: Color, started: std::time::Instant, what: &str) -> ExitCode {
    use std::io::IsTerminal;
    if output.as_os_str() == "-" {
        print!("{text}");
        return ExitCode::SUCCESS;
    }
    if let Err(error) = pipeline::write(output, text) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    let style = render::Style::new(color_on(color, std::io::stderr().is_terminal()));
    eprintln!("{} Wrote {}: {what}, {} in {:.1}s", style.green_mark(), output.display(), check::megabytes(text.len() as u64), started.elapsed().as_secs_f64());
    ExitCode::SUCCESS
}

fn run_decompile(path: PathBuf, output: Option<PathBuf>, split: Option<PathBuf>) -> ExitCode {
    let story = match Story::read(&path) {
        Ok(story) => story,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let started = std::time::Instant::now();
    for note in build::twee::notes(&story) {
        eprintln!("note: {note}");
    }
    if let Some(dir) = split {
        let (twee, js, css) = build::twee::split(&story);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("{}: {e}", dir.display());
            return ExitCode::from(1);
        }
        for (name, text) in [("story.twee", twee), ("story.js", js), ("story.css", css)] {
            if text.trim().is_empty() && name != "story.twee" {
                continue;
            }
            if let Err(e) = std::fs::write(dir.join(name), format!("{}\n", text.trim_end())) {
                eprintln!("{}: {e}", dir.join(name).display());
                return ExitCode::from(1);
            }
        }
        eprintln!("Wrote {}: {} passages in {:.1}s", dir.display(), story.passages.len(), started.elapsed().as_secs_f64());
        return ExitCode::SUCCESS;
    }
    let text = build::twee::story(&story);
    match output {
        Some(file) => write_output(&file, &format!("{}\n", text.trim_end()), Color::Auto, started, &format!("{} passages", story.passages.len())),
        None => {
            println!("{}", text.trim_end());
            ExitCode::SUCCESS
        }
    }
}

fn run_init(dir: Option<PathBuf>, format: &str, name: Option<String>) -> ExitCode {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let (formats, _) = build::formats::all(&build::formats::search_folders());
    let chosen = match build::formats::choose(&formats, Some(format), None, None) {
        Ok(f) => f,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let name = name.unwrap_or_else(|| {
        let full = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        full.file_name().and_then(|n| n.to_str()).map(str::to_string).filter(|n| !n.is_empty()).unwrap_or_else(|| "My Story".into())
    });
    let files = project::starter(&name, &chosen.name, &chosen.version, &build::ifid::new());
    if let Some((file, _)) = files.iter().find(|(file, _)| dir.join(file).exists() && *file != ".gitignore") {
        eprintln!("{} already exists; init makes a new project, so it leaves an existing one alone.", dir.join(file).display());
        return ExitCode::from(2);
    }
    for (file, text) in &files {
        let path = dir.join(file);
        if *file == ".gitignore" && path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, text) {
            eprintln!("{}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    eprintln!("Made a {} {} project in {}: twee.toml and src/.", chosen.name, chosen.version, dir.display());
    let cd = if dir == std::path::Path::new(".") { String::new() } else { format!("cd {} && ", dir.display()) };
    eprintln!("Next: {cd}twee-tools dev    (or twee-tools build)");
    ExitCode::SUCCESS
}

fn run_formats(what: FormatsCommand) -> ExitCode {
    // With a project, its own format folders too, as a build would look.
    let mut folders = build::formats::search_folders();
    if let Ok(Some(project)) = project::Project::find() {
        folders.extend(project.format_paths.iter().flatten().map(|p| project.path(p)).filter(|p| p.is_dir()));
    }
    match what {
        FormatsCommand::List => {
            let (formats, notes) = build::formats::all(&folders);
            let width = formats.iter().map(|f| f.id.len()).max().unwrap_or(2).max(2);
            println!("{:width$}  Name and version", "ID");
            for format in &formats {
                let origin = match &format.origin {
                    build::formats::Origin::BuiltIn => "built in".to_string(),
                    build::formats::Origin::Folder(dir) => dir.display().to_string(),
                };
                let proofing = if format.proofing { ", proofing" } else { "" };
                println!("{:width$}  {} {} ({origin}{proofing})", format.id, format.name, format.version);
            }
            for note in notes {
                eprintln!("note: {note}");
            }
        }
        FormatsCommand::Path => {
            if folders.is_empty() {
                println!(
                    "No storyformats folders found. Formats are searched for in storyformats (or .storyformats, story-formats) beside this program, in your home folder and in the working folder, then in each folder in TWEEGO_PATH."
                );
            }
            for folder in folders {
                println!("{}", folder.display());
            }
        }
        FormatsCommand::Licenses => {
            for (id, _, notice) in build::formats::BUILT_IN {
                println!("==== {id} ====\n\n{}\n", notice.trim());
            }
        }
    }
    ExitCode::SUCCESS
}

fn run_compress(path: PathBuf, out: Option<PathBuf>, options: media::Options, json: bool) -> ExitCode {
    let (story, _) = match load(std::slice::from_ref(&path), &[]) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let out = match (out, options.dry_run) {
        (Some(out), _) => out,
        (None, true) => PathBuf::new(),
        (None, false) => {
            eprintln!("Say where to write the copy with --out, or use --dry-run.");
            return ExitCode::from(2);
        }
    };
    let mut spinner = spinner::Spinner::start("Finding the story's media");
    let result = media::compress_with(&story, &out, &options, &|stage| spinner.set(stage));
    spinner.finish();
    match result {
        Ok(report) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                print!("{}", report.summary());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
