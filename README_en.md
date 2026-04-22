# ncm2mp3

**English | [简体中文](./README.md)**

A Rust CLI that decrypts Netease Cloud Music `.ncm` files back into their original audio (MP3 / FLAC / M4A …), with tags, cover art, parallel batching, and bilingual EN/ZH UI.

![Demo](./docs/demo.gif)

**Inspect metadata (no decryption)**:

```console
$ ncm2mp3 info "范玮琪,张韶涵 - 如果的事.ncm"
File: 范玮琪,张韶涵 - 如果的事.ncm
Size: 9.44 MB
Format: MP3
Bitrate: 320 kbps
Duration: 03:48
Title: 如果的事
Artist: 范玮琪, 张韶涵
Album: Faces Of FanFan
Cover: PNG, 736.62 KB
```

**Single file** (default: preserves input filename):

```console
$ ncm2mp3 "范玮琪,张韶涵 - 如果的事.ncm"
✓ 范玮琪,张韶涵 - 如果的事.ncm -> 范玮琪,张韶涵 - 如果的事.mp3
Done: 1 ok, 0 skipped, 0 failed
```

**Batch decrypt an entire directory** (colored per-file status + count summary):

```console
$ ncm2mp3 ./ncm_library -r -j 8
✓ dkj - Aces.ncm -> dkj - Aces.mp3
✓ emovo - 红.ncm -> emovo - 红.mp3
· Gareth.T - 遇上你之前的我.ncm  (output exists: …mp3)
✗ bogus.ncm: failed to parse …: invalid NCM magic header (expected CTENFDAM)
Done: 2 ok, 1 skipped, 1 failed
```

**Per-song folder + separate cover file** (`-F`):

```console
$ ncm2mp3 "如果的事.ncm" -o ./out -F
$ tree ./out
./out
└── 如果的事/
    ├── 如果的事.mp3     # Embedded ID3v2 tags + cover
    └── cover.png        # Stand-alone cover file
```

## Features

- **No transcoding**: the audio inside an NCM file is already a real MP3/FLAC/M4A/…; this tool just strips the encryption layer
- **Format auto-detection**: picks the real format from the decrypted audio's magic bytes rather than trusting the (sometimes stale) metadata hint
- **Tags & cover**: writes ID3v2 (MP3) or Vorbis Comment (FLAC) tags and embeds the cover image
- **Filename templates**: flexible naming via `--template "{artist}/{album}/{title}"`, or keep the original stem by default
- **Per-song folder layout**: `--folder` drops each song in its own folder and exports the cover as a separate `cover.jpg` / `cover.png`
- **Parallel batching**: `-j N` workers (default: CPU count), `-r` recurses directories
- **Bilingual**: auto-detects from system locale, or override with `-L zh` / `NCM2MP3_LANG=zh`
- **Dry-run preview**: `--dry-run` prints what would be produced, without touching disk
- **Pure Rust, zero runtime deps**: single 1.4 MB static binary, no ffmpeg needed

## Install

### Download a prebuilt binary (easiest)

Grab the archive for your platform from the [Releases page](https://github.com/Johnserf-Seed/ncm2mp3/releases):

- `ncm2mp3-vX.Y.Z-x86_64-pc-windows-msvc.zip` — Windows 64-bit
- `ncm2mp3-vX.Y.Z-aarch64-pc-windows-msvc.zip` — Windows ARM64
- `ncm2mp3-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` — Linux x86_64
- `ncm2mp3-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` — Linux ARM64
- `ncm2mp3-vX.Y.Z-x86_64-apple-darwin.tar.gz` — macOS Intel
- `ncm2mp3-vX.Y.Z-aarch64-apple-darwin.tar.gz` — macOS Apple Silicon

Extract and drop `ncm2mp3(.exe)` somewhere on your PATH.

### Build from source

Requires the [Rust toolchain](https://rustup.rs/) (1.75+).

```bash
git clone https://github.com/Johnserf-Seed/ncm2mp3.git
cd ncm2mp3
cargo build --release
# Binary lands at target/release/ncm2mp3(.exe)
```

### Install on PATH

```bash
cargo install --path crates/ncm-cli
# Now `ncm2mp3` is callable from anywhere
```

## Usage

### Basic

```bash
# Single file, preserves original filename
ncm2mp3 song.ncm

# Specify output directory
ncm2mp3 song.ncm -o ./output

# Custom filename template
ncm2mp3 song.ncm -t "{artist} - {title}"

# Nest by artist/album/title
ncm2mp3 song.ncm -t "{artist}/{album}/{title}"

# Per-song folder with separate cover file
ncm2mp3 song.ncm -F
```

### Batch

```bash
# Process a directory
ncm2mp3 ./ncm_library

# Recurse into subdirectories
ncm2mp3 ./ncm_library -r

# Control parallel worker count
ncm2mp3 ./ncm_library -r -j 8

# Only process FLAC-encoded NCMs
ncm2mp3 ./ncm_library -r --format flac

# Overwrite existing outputs
ncm2mp3 ./ncm_library -r --overwrite
```

### Inspect metadata

```bash
# Single file
ncm2mp3 info song.ncm

# Whole directory
ncm2mp3 info ./ncm_library -r
```

### Extract just the cover art

```bash
# Write cover next to the NCM (auto-picks .jpg or .png by MIME)
ncm2mp3 cover song.ncm
# -> song.jpg or song.png

# Into a dedicated directory
ncm2mp3 cover song.ncm -o ./covers
```

### Watch mode

```bash
# Monitor a directory — new or modified .ncm files are decrypted on the fly
ncm2mp3 watch ./Downloads -o ./Music
# Ctrl-C to stop

# All the usual decrypt knobs work
ncm2mp3 watch ./Downloads -o ./Music -t "{artist}/{album}/{title}" -F
```

**Use case**: keep a terminal running while you download music; new files
land in your target library automatically. A 1-second debouncer merges
editor-style intermediate writes into a single decryption pass.

### Dry-run preview

```bash
ncm2mp3 ./ncm_library -r -t "{artist}/{album}/{title}" --dry-run
```

Prints each output path without writing anything.

### Configuration file

Persist common defaults to a TOML file instead of retyping flags every
invocation. Default paths:

- Linux / macOS: `~/.config/ncm2mp3/config.toml`
- Windows: `%APPDATA%\ncm2mp3\config.toml`

Example:

```toml
# ~/.config/ncm2mp3/config.toml
template = "{artist}/{album}/{title}"
output = "/music/library"
jobs = 8
folder = true
on_conflict = "rename"   # skip (default) / overwrite / rename
recursive = true
format = ["mp3", "flac"]
```

**Precedence**: command-line arguments > config file > built-in defaults.
If the config sets `jobs = 8` and you pass `-j 4`, the final value is 4.

**Other flags**:
- `--config <path>`: load from this TOML file (overrides the default location)
- `--no-config`: skip config loading entirely — use only CLI args + defaults

### Language

```bash
# Command-line flag
ncm2mp3 -L zh song.ncm
ncm2mp3 --lang en song.ncm

# Environment variable (persistent)
export NCM2MP3_LANG=zh    # bash / zsh
$env:NCM2MP3_LANG = "zh"  # PowerShell

# Auto-detected from LANG / LC_ALL by default
# A Chinese-locale system shows Chinese UI out of the box.
```

### Shell completion

```bash
# Bash
ncm2mp3 completion bash > ~/.local/share/bash-completion/completions/ncm2mp3

# Zsh
ncm2mp3 completion zsh > ~/.zfunc/_ncm2mp3
# Make sure ~/.zfunc is on $fpath

# Fish
ncm2mp3 completion fish > ~/.config/fish/completions/ncm2mp3.fish

# PowerShell (append to $PROFILE)
ncm2mp3 completion powershell | Out-String | Invoke-Expression
```

Supported shells: `bash` / `zsh` / `fish` / `powershell` / `elvish`.

## Template placeholders

| Placeholder | Meaning | Example |
|---|---|---|
| `{title}` | Song title | `如果的事` |
| `{artist}` | Artist (multiple joined with `, `) | `范玮琪, 张韶涵` |
| `{album}` | Album name | `Faces Of FanFan` |
| `{format}` | Audio format extension | `mp3` / `flac` |
| `{bitrate}` | Bitrate in kbps | `320` |

`/` or `\` inside the template creates subdirectories. Each placeholder's value is sanitized for filesystem safety (Windows-reserved chars `< > : " / \ | ? *` become `_`).

## Full CLI reference

```
Usage: ncm2mp3 [OPTIONS] <INPUT>
       ncm2mp3 info <INPUT>

Arguments:
  <INPUT>                   .ncm file or a directory of them

Options:
  -o, --output <DIR>        Output directory (default: input's parent)
  -t, --template <TEMPLATE> Filename template (default: preserve input stem)
  -r, --recursive           Recurse into subdirectories
      --format <FMT,...>    Filter by internal format (mp3 / flac / m4a …)
      --no-tag              Don't write tags or embed cover
  -F, --folder              Wrap each song in its own folder + external cover
  -j, --jobs <N>            Parallel worker count (default: CPU cores)
      --overwrite           Overwrite existing outputs instead of skipping
      --dry-run             Preview only, don't write anything
  -v, --verbose             -v for info logs, -vv for debug
  -L, --lang <en|zh>        UI language (default: auto-detected)
  -h, --help                Show help
  -V, --version             Show version
```

## Supported formats

Decryption preserves whatever format is inside the NCM. Detection is by magic bytes:

| Magic | Format | Extension |
|---|---|---|
| `ID3` / `0xFFFB` / `0xFFFA` | MP3 | `.mp3` |
| `fLaC` | FLAC | `.flac` |
| `ftyp` at offset 4 | M4A / AAC | `.m4a` |
| `RIFF...WAVE` | WAV | `.wav` |
| `OggS` | Ogg Vorbis | `.ogg` |

## Project layout

Cargo workspace with two crates:

- **`ncm-core`** — pure decryption library. NCM binary-format parser, NCM custom stream cipher, AES-128-ECB metadata decryption, audio format sniffing. **No CLI deps**, usable standalone.
- **`ncm-cli`** — the CLI front-end. `clap` subcommand dispatch, `lofty` tagging, `rayon` parallelism, i18n.

## Build

```bash
# Dev build (fast)
cargo build

# Release build (~1.4 MB)
cargo build --release

# Full test suite (40+ tests)
cargo test --workspace
```

## How it works

NCM file layout (sequential):

```
Magic 'CTENFDAM' (8B) + Gap (2B)
→ RC4 key length (4B LE) + RC4 key blob  [XOR 0x64 → AES-128-ECB(CORE_KEY) → strip "neteasecloudmusic"]
→ Metadata length (4B LE) + metadata blob [XOR 0x63 → strip "163 key(Don't modify):" → Base64 → AES-128-ECB(META_KEY) → strip "music:" → JSON]
→ CRC32 (4B) + Gap (5B)
→ Cover length (4B LE) + raw cover bytes (JPEG or PNG)
→ Audio data until EOF [NCM's RC4-like stream cipher]
```

The stream cipher is a quirky RC4 variant: standard KSA but a custom PRGA that derives each byte via `S[(S[i] + S[(S[i] + i) & 0xff]) & 0xff]`, indexed directly by byte offset rather than evolving state. This makes streaming chunked decryption trivial — no need to buffer the whole song in memory.

## Credits

NCM format reverse-engineering was a collective effort over the years. Thanks to prior implementations:

- [anonymous5l/ncmdump](https://github.com/anonymous5l/ncmdump) (C++)
- [taurusxin/ncmdump](https://github.com/taurusxin/ncmdump) (Go)
- [nondanee/ncmdump](https://github.com/nondanee/ncmdump) (C)
- [iqiziqi/ncmdump.rs](https://github.com/iqiziqi/ncmdump.rs) (Rust reference)

## License

Licensed under the [Apache License 2.0](./LICENSE). Intended for decrypting music **you legally own** for personal archival. Do not distribute copyrighted audio.
