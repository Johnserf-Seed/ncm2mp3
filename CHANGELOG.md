# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.1] - 2026-04-28

### Added

- **Subcommand typo correction**: `ncm2mp3 inf song.ncm` now infers
  `info`, `comp bash` infers `completion`, etc. Standalone substitution
  typos like `wath` / `covar` get an explicit "Did you mean `watch`?"
  hint. Combines clap's `infer_subcommands(true)` (prefix matches) with
  a Levenshtein-based suggester (substitution typos). Both English and
  Chinese hints. Powered by the lightweight `strsim` crate.
- **Distribution manifests** under `dist/`: a Scoop manifest
  (`dist/scoop/ncm2mp3.json`) and Homebrew formula
  (`dist/homebrew/ncm2mp3.rb`) for one-line installation on Windows /
  macOS / Linux.
- **Auto-sync workflow** (`.github/workflows/update-dist.yml`): fires
  on each Release publish, fetches the 6 SHA256 hashes from the
  release's `.sha256` siblings, splices them + the new version into
  both manifests, and opens a PR for review. No more manual hash
  copy-paste per release.
- **Release archives now ship `.sha256` siblings** (`release.yml` was
  updated to emit them). Useful both for the auto-sync workflow and
  for users who want to verify a manual download with `sha256sum -c`.

### Changed

- **Crate names on crates.io**:
  - `ncm-core` → `ncm2mp3-core` (the `ncm-core` slot was taken by an
    unrelated project). Source code stays the same — `ncm-cli`
    aliases the dep back to `ncm-core` via Cargo's `package = "..."`.
  - `ncm-cli` → `ncm2mp3` (so that `cargo install ncm2mp3` Just Works
    and matches the binary name users actually type).
- **README badges**: crates.io version, docs.rs, downloads, license,
  CI status — all five at the top of both READMEs.

### Tests

- **+28 unit tests in `pipeline.rs`** (was 0): glob exclusion, format
  filter, `--from-file` parsing, conflict resolution (skip/overwrite/
  rename + index probing), output path resolution across template ×
  folder mode × extension swap combinations. Brings total test count
  to 79.

## [0.3.0] - 2026-04-23

### Added

- `--exclude <GLOB>` — skip files matching one or more glob patterns (can
  be repeated). Applied after directory collection and dedup. Backed by
  the `globset` crate; patterns match against the full path string.
- `--limit N` — cap the processed file count. Useful for smoke-testing a
  large library ("try 5 files first, check the output path, then unleash").
- Extended template placeholders:
  - `{bitrate_k}` — kbps with explicit "k" suffix, e.g. `320k`.
  - `{duration}` — total seconds (integer).
  - `{duration_mmss}` — `mm:ss` rendering.
- `--color=auto|always|never` to force-enable or disable ANSI color in
  per-file status lines. Honors the `NO_COLOR` env var (forces never).

### Distribution

- Scoop manifest and Homebrew formula published under `dist/` for
  single-command installs on Windows and macOS.

## [0.2.0] - 2026-04-23

### Added

- `--from-file <list.txt>` to read additional input paths from a text file
  (one path per line, `#` comments and blank lines ignored).
- `--on-conflict skip|overwrite|rename` replaces the boolean `--overwrite`
  with a three-way strategy. `rename` appends `-1`, `-2`, … to the output
  stem until a free path is found. `--overwrite` kept as an alias.
- `cover` subcommand — extract the embedded cover image as `stem.jpg` /
  `stem.png` without decrypting the audio. Supports directory input + `-r`.
- `watch` subcommand — monitor a directory and auto-decrypt new `.ncm`
  files as they appear or get modified. Reuses decrypt pipeline options
  (template, output, format, folder, on-conflict, no-tag). Debounces by
  1s so editor-style intermediate writes merge into a single decryption.
- Runtime stats in the summary line: elapsed wall-clock + per-format
  breakdown (`Done: 10 ok, 0 skipped, 0 failed (1m23s) — MP3:8 FLAC:2`).
- Configuration file support: TOML at `~/.config/ncm2mp3/config.toml`
  (or `%APPDATA%\ncm2mp3\config.toml` on Windows). Precedence is
  **CLI flags > config file > built-in defaults**. Override via `--config
  <path>` or disable entirely via `--no-config`.
- `docs/ARCHITECTURE.md` — byte-level NCM format reference, stream cipher
  math, module layout.
- `docs/demo.tape` — reproducible vhs script for regenerating the README
  demo GIF locally.
- Project hygiene files: `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`.
- `ncm-core` gained comprehensive rustdoc on every public item, with
  `#![warn(missing_docs)]` enforced so CI rejects undocumented additions.

### Changed

- `Cli::input` is now `Option<PathBuf>` to accommodate `--from-file`
  as the sole input source.
- `AudioFormat` gained `#[derive(Hash)]` so it can key the per-format
  stats `HashMap`.
- Bumped seven major-version-stale dependencies across two waves:
  thiserror 1→2, console 0.15→0.16, aes 0.8→0.9, cipher 0.4→0.5,
  ecb 0.1→0.2, lofty 0.21→0.24, dirs 5→6, notify 6→8,
  notify-debouncer-mini 0.4→0.7, toml 0.8→1.

## [0.1.0] - 2026-04-22

Initial release.

### Added

- **`ncm-core` library** — pure decryption primitives, usable as a standalone crate:
  - NCM binary format parser (Magic → RC4 key → metadata → cover → audio)
  - Custom stream cipher (standard RC4 KSA + NCM-specific PRGA, streamable by offset)
  - AES-128-ECB + PKCS7 wrapper for key & metadata segments
  - Audio format auto-detection from magic bytes (MP3, FLAC, M4A, WAV, Ogg)
  - Cover MIME detection (JPEG, PNG)
  - `NcmDecoder` streaming orchestrator with head-sniffing
- **`ncm2mp3` CLI binary**:
  - Default `decrypt` mode (implicit when no subcommand)
  - `info <INPUT>` — inspect headers without decrypting audio
  - `completion <SHELL>` — generate bash/zsh/fish/powershell/elvish completion scripts
  - `-t/--template` filename templates with `{artist} {album} {title} {format} {bitrate}` placeholders
  - `-F/--folder` per-song folder layout with separate `cover.jpg`/`cover.png`
  - `-r/--recursive` directory traversal
  - `--format` filter by internal audio format
  - `--no-tag` opt out of ID3/Vorbis tag writing
  - `--overwrite` / `--dry-run`
  - `-j/--jobs` parallel workers (default: CPU count)
  - `-v`/`-vv` log verbosity
  - `-L/--lang en|zh` UI language, also via `NCM2MP3_LANG` env var, auto-detected from `LANG`/`LC_ALL`
- **i18n** — English and Simplified Chinese for help text, runtime messages, errors
- **Cross-platform releases** — GitHub Actions build 6 target triples on tag push:
  - `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`
  - `x86_64-apple-darwin` / `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc` / `aarch64-pc-windows-msvc`
- **CI** — format + clippy + test on Ubuntu/macOS/Windows for every push and PR
- **Documentation** — bilingual `README.md` (Chinese) + `README_en.md` (English)
- **License** — Apache-2.0

[Unreleased]: https://github.com/Johnserf-Seed/ncm2mp3/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.1
[0.3.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.0
[0.2.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.2.0
[0.1.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.1.0
