//! The version of twee-tools a project pins in its twee.toml, `twee-tools = "0.1.3"`: when the one running isn't that
//! version, it downloads that release once into a cache, checks it against the release's checksums, and runs it with the
//! same arguments. So everyone working on a project, and its CI, builds with the same twee-tools, whichever one each has
//! installed.
//!
//! This runs before the command line is parsed, and reads twee.toml as plain TOML, so options and keys that only the
//! pinned version knows still reach it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The first version that knows the `twee-tools` key: an older one would refuse its own twee.toml.
pub(crate) const PINS_SINCE: &str = "0.1.1";

/// Set to run this copy whatever the project pins; the pinned copy is started with it set, so it doesn't follow again.
const NO_PIN: &str = "TWEE_TOOLS_NO_PIN";

/// Commands that aren't about the project, so never follow its pin.
const OWN: &[&str] = &["upgrade", "init"];

/// If the project pins another version, runs that one and gives its exit code; `None` carries on here.
pub(crate) fn follow() -> Option<ExitCode> {
    if std::env::var_os(NO_PIN).is_some_and(|v| !v.is_empty()) {
        return None;
    }
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let (directory, command) = scan(&args);
    if command.as_deref().is_some_and(|c| OWN.contains(&c)) {
        return None;
    }
    let start = std::env::current_dir().ok()?.join(directory.unwrap_or_default());
    let (file, pinned) = match pinned(&start) {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(error) => {
            eprintln!("{error}");
            return Some(ExitCode::from(2));
        }
    };
    if pinned == env!("CARGO_PKG_VERSION") {
        return None;
    }
    match run(&file, &pinned, &args) {
        Ok(code) => Some(code),
        Err(error) => {
            eprintln!("{error}");
            Some(ExitCode::from(2))
        }
    }
}

/// The folder `-C` names, and the command: the first argument that isn't an option.
fn scan(args: &[OsString]) -> (Option<PathBuf>, Option<String>) {
    let (mut directory, mut command) = (None, None);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        if text == "-C" || text == "--directory" {
            directory = args.next().map(PathBuf::from);
        } else if let Some(dir) = text.strip_prefix("--directory=") {
            directory = Some(PathBuf::from(dir));
        } else if let Some(dir) = text.strip_prefix("-C").filter(|d| !d.is_empty()) {
            directory = Some(PathBuf::from(dir));
        } else if !text.starts_with('-') && command.is_none() {
            command = Some(text.into_owned());
        }
    }
    (directory, command)
}

/// The nearest twee.toml from `start`, and the version it pins, if it pins one.
fn pinned(start: &Path) -> Result<Option<(PathBuf, String)>, String> {
    let Some(file) = start.ancestors().map(|dir| dir.join("twee.toml")).find(|file| file.is_file()) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    // Plain TOML: a pinned newer version may have keys this one doesn't know, and they're its to check.
    let table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", file.display()))?;
    let Some(value) = table.get("twee-tools") else {
        return Ok(None);
    };
    let version = value.as_str().map(|v| v.trim().trim_start_matches('v').to_string()).filter(|v| story::parse_version(v).is_some());
    let Some(version) = version else {
        return Err(format!("{}: twee-tools = {value} isn't a version: give it as \"x.y.z\".", file.display()));
    };
    // This very version knows the key, whatever its number; an older one would refuse its own twee.toml.
    if version != env!("CARGO_PKG_VERSION") && !crate::project::at_least(&version, PINS_SINCE) {
        return Err(format!("{}: twee-tools {version} doesn't know the twee-tools key, so it can't be pinned: pin {PINS_SINCE} or later.", file.display()));
    }
    Ok(Some((file, version)))
}

/// Runs the pinned version, downloading it first if it isn't in the cache yet.
fn run(file: &Path, version: &str, args: &[OsString]) -> Result<ExitCode, String> {
    let target = crate::upgrade::target().ok_or_else(|| {
        format!("{} pins twee-tools {version}, and there's no release build for this system: build that version from source.", file.display())
    })?;
    let cache = cache().ok_or("couldn't find a folder to keep downloaded versions in: set TWEE_TOOLS_CACHE_DIR")?;
    let folder = cache.join(version);
    let binary = folder.join(format!("twee-tools{}", std::env::consts::EXE_SUFFIX));
    if !binary.is_file() {
        eprintln!("{} pins twee-tools {version}: downloading it, once.", file.display());
        let bytes = crate::upgrade::Release::new(Some(version)).binary(target)?;
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        // Through a new file and a rename, so a download cut short never leaves a broken copy behind.
        let partial = folder.join(format!(".download-{}", std::process::id()));
        std::fs::write(&partial, bytes).map_err(|e| format!("{}: {e}", partial.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("{}: {e}", partial.display()))?;
        }
        std::fs::rename(&partial, &binary).map_err(|e| format!("{}: {e}", binary.display()))?;
    }
    let mut command = std::process::Command::new(&binary);
    command.args(args).env(NO_PIN, "1");
    #[cfg(unix)]
    {
        // In place of this process, so signals (Ctrl+C in `dev`) and the exit code are the pinned one's own.
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(format!("couldn't run twee-tools {version} ({}): {error}", binary.display()))
    }
    #[cfg(not(unix))]
    {
        let status = command.status().map_err(|e| format!("couldn't run twee-tools {version} ({}): {e}", binary.display()))?;
        Ok(ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8))
    }
}

/// Where downloaded versions are kept: TWEE_TOOLS_CACHE_DIR, else the system's cache folder.
fn cache() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = var("TWEE_TOOLS_CACHE_DIR") {
        return Some(dir);
    }
    if cfg!(windows) {
        return var("LOCALAPPDATA").map(|dir| dir.join("twee-tools").join("versions"));
    }
    if cfg!(target_os = "macos") {
        return var("HOME").map(|home| home.join("Library/Caches/twee-tools/versions"));
    }
    var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache"))).map(|dir| dir.join("twee-tools").join("versions"))
}
