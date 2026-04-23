# ncm2mp3-core

Pure-Rust library for decrypting Netease Cloud Music `.ncm` files. No
network, no transcoding — parse the container, run the crypto, stream
the original audio (MP3 / FLAC / M4A / …) out.

Used by the [`ncm2mp3`](https://github.com/Johnserf-Seed/ncm2mp3) CLI,
but deliberately kept CLI-free so other tools can depend on it.

> The `ncm-core` / `ncm_core` name on crates.io was taken by an
> unrelated project, so the published name is `ncm2mp3-core`. Inside
> the CLI's own workspace this crate is still aliased as `ncm_core` via
> a [`package = "..."`](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#renaming-dependencies-in-cargotoml)
> rename, so the source doesn't have to spell out the longer name.

## Quick start

```toml
[dependencies]
ncm2mp3-core = "0.3"
```

```rust,no_run
use ncm2mp3_core::NcmDecoder;

fn main() -> ncm2mp3_core::Result<()> {
    let (mut decoder, headers) = NcmDecoder::open("song.ncm")?;
    println!("title: {}", headers.metadata.title);
    println!("artist: {}", headers.metadata.artists_joined(", "));

    let mut out = std::fs::File::create("song.mp3")?;
    decoder.decode_to_writer(&mut out)?;
    Ok(())
}
```

Prefer the short `ncm_core` name in your own code? Alias it in your
`Cargo.toml`:

```toml
[dependencies]
ncm_core = { package = "ncm2mp3-core", version = "0.3" }
```

Then `use ncm_core::NcmDecoder;` works.

The format itself is documented at
<https://github.com/Johnserf-Seed/ncm2mp3/blob/main/docs/ARCHITECTURE.md>
— byte-level layout, the AES keys, the quirky RC4-variant stream
cipher, and the rationale for each transform layer.

## What's in the public API

- [`NcmDecoder`] / [`NcmHeaders`] — one-shot open, streaming decrypt
- [`NcmMetadata`] — title / artist / album / bitrate / duration (after
  JSON-decoding the encrypted metadata blob)
- [`Cover`] — raw cover image bytes + sniffed [`CoverMime`]
- [`AudioFormat`] — detected or declared audio format
- [`NcmError`] — one enum for every failure mode; splits format-level
  from crypto/transform failures
- [`crypto`] module — `aes128_ecb_decrypt` and `NcmStreamCipher` as
  standalone primitives if you're writing a different parser

## License

Apache-2.0. See the [`LICENSE`](../../LICENSE) file.
