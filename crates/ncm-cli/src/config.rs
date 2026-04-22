//! TOML configuration file support.
//!
//! Users can persist common decrypt-mode defaults (template / output / jobs
//! / folder / …) so they don't retype them on every invocation. Precedence
//! at merge time is **command line > config file > built-in defaults**.
//!
//! Default location:
//! - Linux / macOS: `~/.config/ncm2mp3/config.toml` (via `dirs::config_dir`)
//! - Windows: `%APPDATA%\ncm2mp3\config.toml`
//!
//! Override the lookup via `--config <path>`; disable loading entirely with
//! `--no-config`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Deserialized shape of `config.toml`. Every field is optional — a missing
/// entry means "don't touch the CLI/built-in default".
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    /// Default filename template (same syntax as `--template`).
    pub template: Option<String>,
    /// Default output directory (same as `--output`).
    pub output: Option<PathBuf>,
    /// Default parallel worker count (same as `--jobs`).
    pub jobs: Option<usize>,
    /// Default folder-mode flag (same as `--folder`).
    pub folder: Option<bool>,
    /// Default tag-writing opt-out (same as `--no-tag`).
    pub no_tag: Option<bool>,
    /// Default conflict strategy: `"skip"`, `"overwrite"`, or `"rename"`.
    pub on_conflict: Option<String>,
    /// Default recursive flag (same as `--recursive`).
    pub recursive: Option<bool>,
    /// Default UI language: `"en"` or `"zh"`. Accepted in the schema so
    /// users don't trip `deny_unknown_fields`, but not actually consumed
    /// yet — i18n is initialized before the config is loaded. Use the
    /// `NCM2MP3_LANG` env var or `-L`/`--lang` instead.
    #[allow(dead_code)]
    pub lang: Option<String>,
    /// Default format filters (same as repeated `--format`).
    pub format: Option<Vec<String>>,
}

/// Return the platform-appropriate default config path, or `None` if the
/// home / config directory can't be resolved.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("ncm2mp3").join("config.toml"))
}

/// Load config from an explicit path, erroring if the file is missing or
/// malformed.
pub fn load_from(path: &Path) -> Result<FileConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    toml::from_str(&text)
        .with_context(|| format!("failed to parse config file: {}", path.display()))
}

/// Load the default config if it exists. Returns `Ok(Default::default())`
/// when the file isn't present (absent config is a valid state, not an
/// error). Only the case of "file exists but malformed" is surfaced as an
/// error — callers probably want to know about that.
pub fn load_default_if_present() -> Result<FileConfig> {
    match default_path() {
        Some(p) if p.exists() => load_from(&p),
        _ => Ok(FileConfig::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_fields() {
        let toml_src = r#"
            template = "{artist}/{album}/{title}"
            output = "./out"
            jobs = 4
            folder = true
            no_tag = false
            on_conflict = "rename"
            recursive = true
            lang = "zh"
            format = ["mp3", "flac"]
        "#;
        let cfg: FileConfig = toml::from_str(toml_src).unwrap();
        assert_eq!(cfg.template.as_deref(), Some("{artist}/{album}/{title}"));
        assert_eq!(cfg.jobs, Some(4));
        assert_eq!(cfg.folder, Some(true));
        assert_eq!(cfg.on_conflict.as_deref(), Some("rename"));
        assert_eq!(cfg.lang.as_deref(), Some("zh"));
        assert_eq!(
            cfg.format.as_deref(),
            Some(&["mp3".to_string(), "flac".to_string()][..])
        );
    }

    #[test]
    fn partial_config_keeps_other_fields_none() {
        let cfg: FileConfig = toml::from_str(r#"jobs = 2"#).unwrap();
        assert_eq!(cfg.jobs, Some(2));
        assert!(cfg.template.is_none());
        assert!(cfg.folder.is_none());
    }

    #[test]
    fn unknown_field_is_rejected() {
        // `deny_unknown_fields` keeps typos loud.
        let result: Result<FileConfig, _> = toml::from_str(r#"templte = "oops""#);
        assert!(result.is_err());
    }
}
