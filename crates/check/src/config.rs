//! Optional settings: the custom macros a story uses that the checker can't see, in the format Twee 3 Language Tools reads
//! (`*.twee-config.yml`), and story variables something outside the story sets.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default, Clone)]
pub struct Config {
    /// Macro name to whether it's a container.
    pub macros: HashMap<String, bool>,
    pub globals: HashSet<String>,
    /// Finding codes never reported (`unused-media`).
    pub ignore: HashSet<String>,
    /// A severity for a code, over the checker's own (`dead-end` as an error).
    pub severity: HashMap<String, crate::Severity>,
}

#[derive(Deserialize)]
struct File {
    #[serde(rename = "sugarcube-2", default)]
    sugarcube: Option<Format>,
    #[serde(default)]
    globals: Vec<String>,
}

#[derive(Deserialize)]
struct Format {
    #[serde(default)]
    macros: HashMap<String, MacroDef>,
}

#[derive(Deserialize, Default)]
struct MacroDef {
    #[serde(default)]
    container: bool,
}

impl Config {
    /// Reads every `*.twee-config.yml` (or `.yaml`) in a folder, not below it.
    pub fn find(dir: &Path) -> Result<Config, String> {
        let mut config = Config::default();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(config);
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.ends_with(".twee-config.yml") || name.ends_with(".twee-config.yaml") {
                config.add(&path)?;
            }
        }
        Ok(config)
    }

    pub fn add(&mut self, path: &Path) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let file: File = serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(format) = file.sugarcube {
            for (name, def) in format.macros {
                self.macros.insert(name, def.container);
            }
        }
        self.globals.extend(file.globals.into_iter().map(|g| g.trim_start_matches('$').to_string()));
        Ok(())
    }
}
