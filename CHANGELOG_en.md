# Changelog

**English | [简体中文](./CHANGELOG.md)**

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **`scoop install` failed on Windows ARM64**
  ([#8](https://github.com/Johnserf-Seed/ncm2mp3/pull/8)). Each Windows
  release zip wraps its files in an `ncm2mp3-<tag>-<target>/` folder,
  but the Scoop manifest's `bin` hardcoded the path inside the x86_64
  zip. The ARM64 zip has no such path, so the install aborted while
  creating the shim (`Can't shim ... File doesn't exist.`). The manifest
  now sets `extract_dir` per architecture and `bin` is plain
  `ncm2mp3.exe`; `update-dist.yml` writes both `extract_dir` values when
  it syncs a new release. The bug dates back to when the Scoop manifest
  was added. If an install failed before, just re-run `scoop install`;
  Scoop cleans up the failed attempt automatically.

### Security

All of these were found by GitHub code scanning.

- **Output files were written through symlinks, and parallel jobs could
  corrupt each other's output.** Decrypted audio, `--folder` mode's
  `cover.jpg` / `cover.png`, and covers exported by the `cover`
  subcommand were all written in a way that follows symlinks, and the
  "does the output exist?" check was a separate step from creating the
  file. Anyone who could put files in the input or output directory could
  plant (or race in after the check) a symlink under the output name and
  make ncm2mp3 truncate and overwrite whatever it pointed to, with your
  permissions. When two inputs mapped to the same output name (e.g. via a
  template), parallel jobs wrote it at the same time: both reported `✓`,
  and the file ended up with both songs' data mixed together. Now the
  `skip` (default) and `rename` policies check and create in one atomic
  "create only if absent" step, which also treats a symlink there
  (dangling or not) as existing. `--overwrite` and the `--folder` cover
  write a temporary file in the same directory and rename it into place,
  which replaces the directory entry instead of writing through a link.
  Tags are written through the already-open file handle instead of
  reopening the path. A conversion that fails now also deletes its
  half-written output, so the next run doesn't skip it as existing.
  Behavior change: `--overwrite` now replaces a symlink at the
  destination with a regular file instead of writing to its target.
- **A crafted NCM file could crash the whole process.** A key blob that
  decrypts to just the `neteasecloudmusic` prefix, with no key after it,
  tripped an assertion in the stream cipher. Release builds use
  `panic = "abort"`, so a batch run died on the spot (exit code 134) and
  the remaining files were never processed. Such a file is now rejected
  as an invalid key and only that file fails. In `ncm2mp3-core`,
  `NcmError::InvalidKeyPrefix` now covers this case too, and its message
  is now
  `invalid RC4 key blob (expected 'neteasecloudmusic' followed by the key)`.
- **Header segment length limits were far too high.** The key, metadata
  and cover segments shared a 64 MiB limit, while real keys and metadata
  are a hundred-odd bytes to a few KiB. Decryption briefly holds several
  copies of these segments, so one crafted file could take hundreds of MiB, and
  several processed in parallel could abort the process on a failed
  allocation; the parser also allocated the declared length up front, even
  when the file was much shorter. The key segment is now capped at 64 KiB
  and the metadata segment at 1 MiB (new in `ncm2mp3-core`:
  `format::MAX_KEY_LEN` / `format::MAX_META_LEN`); covers keep 64 MiB.
  Buffers grow only as data is actually read.
- **Terminal control characters in metadata and file names were printed
  verbatim.** `info` printed the NCM metadata's title, artist and album
  as-is, and file names appeared as-is in every per-file status and error
  line. ANSI / OSC escape sequences embedded in them were executed by the
  terminal, which could spoof what's on screen (clear it, fake a `Title:`
  row) or trigger terminal features (e.g. rewrite the clipboard via
  OSC 52). Control characters and bidirectional-text controls in that text
  are now shown as escapes like `\u{1b}`. Tag contents and output file
  names are unchanged.
- **Script injection in `update-dist.yml`.** The workflow spliced the tag
  name (`workflow_run.head_branch`, or the `tag` input of a manual run)
  straight into a bash script, so a value containing `$(...)`, `;` and the
  like would run as commands, in a job holding `contents: write` and
  `pull-requests: write`. Exploiting it took the ability to push a tag or
  trigger the workflow, i.e. write access to the repository. The tag is
  now passed through an environment variable and must look like `v1.2.3`
  (optionally with a suffix such as `-rc.1`); anything else fails the run.

### Tests

- 39 new tests covering: a symlink (including a dangling one) planted at
  the output under each conflict policy, another job still writing the
  same output name, `--overwrite`'s atomic replace and failure cleanup,
  tagging through the file handle, a key blob holding only the prefix,
  per-segment length limits and truncated segments, and terminal
  control-character escaping. Test count goes from 94 to 133, of which
  the 12 symlink tests run on Unix only.

## [0.3.2] - 2026-10-04

### Fixed

- **NCM files from NetEase Cloud Music 3.x decrypted to noise**
  ([#3](https://github.com/Johnserf-Seed/ncm2mp3/issues/3)). Newer
  clients reserve padding after the cover image (typically a zero-length
  image in a non-empty cover frame), and the parser treated that padding
  as the start of the audio, misaligning everything after it. The
  padding is now skipped using the cover frame length — the same fix as
  [taurusxin/ncmdump#26](https://github.com/taurusxin/ncmdump/issues/26).
  Files from older clients have no padding and decode byte-for-byte as
  before. If you already converted such files with an earlier version,
  re-run with `--overwrite`; otherwise they're skipped because the
  output exists.
- **Unrecognizable output is no longer reported as success**
  ([#3](https://github.com/Johnserf-Seed/ncm2mp3/issues/3)). When
  the decrypted audio head matches no known format (MP3 / FLAC / M4A /
  WAV / OGG), the file now counts as failed (`✗`, exit code 1) and
  nothing is written; previously the noise was written under the
  metadata's declared format with a `✓`. `info` now shows the sniffed
  format too, e.g. `unknown (declared: MP3)`.
- **A `.` in the file name truncated the output name**
  ([#4](https://github.com/Johnserf-Seed/ncm2mp3/issues/4)).
  `artist - song name . xxx.ncm` used to come out as
  `artist - song name .flac`; now only the extension changes, giving
  `artist - song name . xxx.flac`. Template-generated names (e.g. a
  `Mr. Brightside` title) and `--folder` mode had the same bug and are
  fixed too.

### Deprecated

- `ncm2mp3-core`: `parser::skip_crc_gap` and `parser::read_cover` don't
  handle cover frame padding; use the new `parser::skip_crc` +
  `parser::read_cover_frame` instead. `NcmDecoder` has switched over.

### Tests

- 15 new tests covering padded and image-less cover frames, the 3.x
  client file layout (144-byte key segment), no output on unrecognized
  audio, and dotted names across every output-path mode. Total test count
  goes from 79 to 94.

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

[Unreleased]: https://github.com/Johnserf-Seed/ncm2mp3/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.2
[0.3.1]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.1
[0.3.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.3.0
[0.2.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.2.0
[0.1.0]: https://github.com/Johnserf-Seed/ncm2mp3/releases/tag/v0.1.0
