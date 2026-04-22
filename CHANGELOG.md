# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/Johnserf-Seed/ncm2mp3/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.1.0
