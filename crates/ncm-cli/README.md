# ncm2mp3

Command-line tool that decrypts Netease Cloud Music `.ncm` files back into
their original audio format (MP3 / FLAC / M4A / …), with ID3v2 / Vorbis
Comment tagging, cover art embedding, parallel batching, a directory
watch mode, shell completions, and a bilingual English / Chinese UI.

Full documentation, screenshots, usage examples, and architecture notes
live in the workspace root:

- **English**: <https://github.com/Johnserf-Seed/ncm2mp3/blob/main/README_en.md>
- **简体中文**: <https://github.com/Johnserf-Seed/ncm2mp3/blob/main/README.md>
- **Architecture**: <https://github.com/Johnserf-Seed/ncm2mp3/blob/main/docs/ARCHITECTURE.md>

## Install

### Via cargo

```bash
cargo install ncm2mp3
```

### Prebuilt binaries

Grab the archive for your platform from the [Releases page](https://github.com/Johnserf-Seed/ncm2mp3/releases).
Supported targets: Windows x64 / ARM64, macOS Intel / Apple Silicon,
Linux x86_64 / ARM64.

### Via package managers

```bash
# Homebrew (macOS / Linux)
brew install --formula https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/homebrew/ncm2mp3.rb

# Scoop (Windows)
scoop install https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/scoop/ncm2mp3.json
```

## Quick look

```console
$ ncm2mp3 info song.ncm
File: song.ncm
Size: 9.44 MB
Format: MP3
Bitrate: 320 kbps
Duration: 03:48
Title: ...
Artist: ...
Album: ...
Cover: PNG, 736.62 KB

$ ncm2mp3 ./library -r -t "{artist}/{album}/{title}" -j 8
✓ dkj - Aces.ncm -> Aces/dkj/Aces.mp3
✓ emovo - 红.ncm -> ...
Done: 50 ok, 0 skipped, 0 failed (12.3s) — MP3:48 FLAC:2
```

For the full CLI reference see `ncm2mp3 --help` after installation, or
the [README](https://github.com/Johnserf-Seed/ncm2mp3) on GitHub.

## Library

The decryption engine is available as a standalone crate:

```toml
[dependencies]
ncm2mp3-core = "0.3"
```

See [ncm2mp3-core on crates.io](https://crates.io/crates/ncm2mp3-core)
or [docs.rs/ncm2mp3-core](https://docs.rs/ncm2mp3-core).

## License

Apache-2.0. See the [LICENSE](https://github.com/Johnserf-Seed/ncm2mp3/blob/main/LICENSE) file.
