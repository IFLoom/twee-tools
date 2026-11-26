//! `twee-tools upgrade`: installs the latest release (or a given one) in place of the running program, after checking
//! it against the release's checksums. A copy that Homebrew, Scoop or `cargo install` put there is theirs to upgrade, so
//! for those it says how instead. Besides this, twee-tools goes online only to fetch the version a project pins (`pin.rs`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

const REPO: &str = "https://github.com/IFLoom/twee-tools";

/// What `upgrade` was asked to do.
pub(crate) struct Options {
    /// A version to install instead of the latest, newer or older.
    pub version: Option<String>,
    /// Only say whether there's a newer version.
    pub check: bool,
}

/// Upgrades, or says why not: the line to print on success, or the error.
pub(crate) fn run(options: Options) -> Result<String, String> {
    let current = env!("CARGO_PKG_VERSION");
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| format!("couldn't find this program's file: {e}"))?;
    if !options.check
        && let Some(how) = managed(&exe)
    {
        return Err(how);
    }
    let target = target().ok_or("there's no release build for this system: build twee-tools from source instead")?;
    let wanted = options.version.as_deref().map(|v| v.trim_start_matches('v').to_string());
    if let Some(version) = &wanted
        && story::parse_version(version).is_none()
    {
        return Err(format!("{version} isn't a version: give it as x.y.z"));
    }
    let release = Release::new(wanted.as_deref());

    // The version to install: the one asked for, else the latest release's.
    let latest = match &wanted {
        Some(version) => version.clone(),
        None => {
            let text = String::from_utf8_lossy(&release.get("VERSION")?).trim().trim_start_matches('v').to_string();
            story::parse_version(&text).ok_or_else(|| format!("the release's VERSION file says {text:?}, which isn't a version"))?;
            text
        }
    };
    let newer = story::parse_version(&latest) > story::parse_version(current);
    if options.check {
        return Ok(if newer {
            format!("twee-tools {latest} is out (this is {current}): `twee-tools upgrade` installs it. What's new: {REPO}/releases/tag/v{latest}")
        } else {
            format!("twee-tools {current} is the latest.")
        });
    }
    if wanted.is_none() && !newer {
        return Ok(format!("twee-tools {current} is the latest."));
    }
    if latest == current {
        return Ok(format!("twee-tools {current} is already installed."));
    }

    let binary = release.binary(target)?;

    // Beside this program (so the swap is a rename on one disk), tried, then swapped in.
    let folder = exe.parent().ok_or("couldn't find this program's folder")?;
    let new = folder.join(format!(".twee-tools-upgrade-{}{}", std::process::id(), std::env::consts::EXE_SUFFIX));
    let result = install(&new, &binary, &latest, &exe);
    let _ = std::fs::remove_file(&new);
    result?;
    Ok(format!("Upgraded twee-tools {current} to {latest}. What's new: {REPO}/releases/tag/v{latest}"))
}

/// A release's files: a given version's, else the latest's, from GitHub, or from TWEE_TOOLS_DOWNLOAD_URL (a mirror, or a
/// test's server).
pub(crate) struct Release {
    base: String,
    version: Option<String>,
    agent: ureq::Agent,
}

impl Release {
    pub(crate) fn new(version: Option<&str>) -> Release {
        let base = match (std::env::var("TWEE_TOOLS_DOWNLOAD_URL").ok().filter(|u| !u.is_empty()), version) {
            (Some(url), _) => url.trim_end_matches('/').to_string(),
            (None, Some(version)) => format!("{REPO}/releases/download/v{version}"),
            (None, None) => format!("{REPO}/releases/latest/download"),
        };
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(300)))
            .user_agent(concat!("twee-tools/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Release { base, version: version.map(str::to_string), agent }
    }

    /// One of the release's files.
    pub(crate) fn get(&self, name: &str) -> Result<Vec<u8>, String> {
        let url = format!("{}/{name}", self.base);
        let mut response = self.agent.get(&url).call().map_err(|e| match (e, &self.version) {
            (ureq::Error::StatusCode(404), Some(version)) => format!("{url} isn't there: is {version} a released version?"),
            (ureq::Error::StatusCode(404), None) => format!("{url} isn't there: there's no published release to upgrade to yet"),
            (e, _) => format!("couldn't download {url}: {e}"),
        })?;
        response.body_mut().with_config().limit(256 * 1024 * 1024).read_to_vec().map_err(|e| format!("couldn't download {url}: {e}"))
    }

    /// The release's binary for a system, checked against the release's checksums.
    pub(crate) fn binary(&self, target: &str) -> Result<Vec<u8>, String> {
        let name = format!("twee-tools-{target}{}", std::env::consts::EXE_SUFFIX);
        let sums = String::from_utf8_lossy(&self.get("SHA256SUMS")?).into_owned();
        let expected = sums
            .lines()
            .filter_map(|line| line.split_once(char::is_whitespace))
            .find(|(_, file)| file.trim().trim_start_matches('*') == name)
            .map(|(sum, _)| sum.to_ascii_lowercase())
            .ok_or_else(|| format!("the release's SHA256SUMS doesn't list {name}"))?;
        let binary = self.get(&name)?;
        let actual: String = Sha256::digest(&binary).iter().map(|b| format!("{b:02x}")).collect();
        if actual != expected {
            return Err(format!("{name} doesn't match its checksum: the download is damaged or not the release's. Nothing was changed."));
        }
        Ok(binary)
    }
}

/// Writes the new binary, checks that it runs and is the version expected, then swaps it in for `exe`.
fn install(new: &Path, binary: &[u8], version: &str, exe: &Path) -> Result<(), String> {
    let denied = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            format!("can't write to {}: run the upgrade as the user that installed twee-tools", exe.parent().unwrap_or(exe).display())
        } else {
            format!("{}: {e}", new.display())
        }
    };
    std::fs::write(new, binary).map_err(denied)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755)).map_err(denied)?;
    }
    let output = std::process::Command::new(new).arg("--version").output().map_err(|e| format!("the downloaded twee-tools doesn't start: {e}"))?;
    let said = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !said.contains(version) {
        return Err(format!("the downloaded twee-tools says {:?}, not {version}. Nothing was changed.", said.trim()));
    }
    self_replace::self_replace(new).map_err(denied)
}

/// How to upgrade a copy that a package manager or `cargo` installed, which `upgrade` leaves alone: `None` for one the
/// install script or a download put there.
fn managed(exe: &Path) -> Option<String> {
    let path = exe.to_string_lossy().replace('\\', "/").to_lowercase();
    let has = |part: &str| path.contains(part);
    if has("/cellar/") || has("/homebrew/") || has("/linuxbrew/") {
        return Some("This twee-tools came from Homebrew: upgrade it with `brew upgrade twee-tools`.".into());
    }
    if has("/node_modules/") {
        return Some("This twee-tools came from npm: upgrade it with `npm install @ifloom/twee-tools@latest`, in the project that has it.".into());
    }
    if has("/scoop/") {
        return Some("This twee-tools came from Scoop: upgrade it with `scoop update twee-tools`.".into());
    }
    let cargo_bin = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|home| PathBuf::from(home).join(".cargo")))
        .map(|home| home.join("bin"));
    if cargo_bin.and_then(|bin| bin.canonicalize().ok()).is_some_and(|bin| exe.parent() == Some(bin.as_path())) {
        return Some(format!("This twee-tools came from `cargo install`: upgrade it with `cargo install --git {REPO} twee-tools --force`."));
    }
    if has("/target/release/") || has("/target/debug/") {
        return Some("This twee-tools was built from source: pull the latest and build it again.".into());
    }
    None
}

/// The name of this system's release build, as the release names its files.
pub(crate) fn target() -> Option<&'static str> {
    if cfg!(windows) {
        // One build, for x64, which Windows on Arm also runs.
        return cfg!(any(target_arch = "x86_64", target_arch = "aarch64")).then_some("x86_64-pc-windows-msvc");
    }
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}
