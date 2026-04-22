# Architecture

Technical notes on the NCM file format and how this codebase maps onto it. Intended for contributors and the curious.

For the user-facing overview, see the [README](../README.md).

---

## 1. Background

"NCM" is the container format used by Netease Cloud Music's desktop app to wrap downloaded audio in a transport-layer-only DRM scheme. The audio payload inside is a *completely ordinary* MP3, FLAC, M4A, etc. file — the format buys the vendor two things:

1. **A recognizable mime that ordinary players reject.** No music app opens `.ncm` directly, so you're nudged back into the Netease client.
2. **A per-file RC4-derived stream cipher over the audio bytes.** Cheap enough to decrypt on playback on a phone, expensive enough that casually renaming the file to `.mp3` gives you noise.

The encryption isn't cryptographically ambitious — both the core AES key and the metadata AES key are **hard-coded constants** that have been public since 2016. Our job is just to parse the container and reverse the transforms in order.

---

## 2. On-disk layout

All multi-byte integers are little-endian. All offsets below are relative to the start of the file.

| Field                  | Size      | Description                                                                       |
|------------------------|-----------|-----------------------------------------------------------------------------------|
| Magic                  | 8 B       | `43 54 45 4E 46 44 41 4D` (ASCII `CTENFDAM`)                                      |
| Gap                    | 2 B       | Unused. Skip.                                                                     |
| `key_len`              | 4 B u32   | Length of the RC4-key blob that follows.                                          |
| RC4 key blob           | `key_len` | Encrypted RC4 key (see §3.1)                                                      |
| `meta_len`             | 4 B u32   | Length of the metadata blob that follows. May be `0`.                             |
| Metadata blob          | `meta_len`| Encrypted JSON metadata (see §3.2) — omit if `meta_len == 0`                      |
| CRC32                  | 4 B u32   | CRC of something. Not validated by this implementation.                            |
| Gap                    | 5 B       | Unused. Skip.                                                                     |
| `cover_len`            | 4 B u32   | Length of the cover blob. May be `0` (no cover).                                  |
| Cover blob             | `cover_len`| Raw JPEG or PNG bytes — no encryption. Sniff MIME from magic bytes.              |
| Audio stream           | to EOF    | RC4-variant-encrypted audio. Decrypt byte-by-byte by absolute offset (see §3.3). |

Implemented in [`crates/ncm-core/src/parser.rs`](../crates/ncm-core/src/parser.rs) and orchestrated by [`crates/ncm-core/src/decoder.rs`](../crates/ncm-core/src/decoder.rs).

Constants for Magic, both AES keys, XOR masks, and prefix strings live in [`crates/ncm-core/src/format.rs`](../crates/ncm-core/src/format.rs).

---

## 3. Cryptographic transforms

Three different transforms are layered, one per segment. The third one (§3.3) is the stream cipher applied to the audio bytes.

### 3.1. RC4 key blob

The payload after `key_len` goes through:

```text
ciphertext
    │
    ├─ XOR 0x64  (each byte)
    │
    ├─ AES-128-ECB decrypt,  key = CORE_KEY = b"hzHRAmso5kInbaxW"
    │    (PKCS#7 padding; the crate blob is always a multiple of 16 bytes)
    │
    └─ strip leading b"neteasecloudmusic"  (17 bytes)
       ──▶ remaining bytes are the RC4 key material for §3.3.
```

If the prefix strip fails, the file is either corrupted or not a real NCM. Implemented as [`parser::read_rc4_key`](../crates/ncm-core/src/parser.rs).

### 3.2. Metadata blob

More transforms than the key, because the payload is JSON:

```text
ciphertext
    │
    ├─ XOR 0x63  (each byte)
    │
    ├─ strip leading b"163 key(Don't modify):"  (22 bytes of ASCII)
    │    ↑
    │    Easy to miss. A parser that skips this step will try to
    │    base64-decode starting at "163 key..." and blow up on
    │    the first space character. (See fix commit 8de3979.)
    │
    ├─ Base64 decode
    │
    ├─ AES-128-ECB decrypt,  key = META_KEY = b"#14ljk_!\]&0U<'("
    │    (PKCS#7 padding)
    │
    ├─ strip leading b"music:"  (6 bytes)
    │
    └─ UTF-8 + JSON parse  ──▶  RawMetadata
```

`RawMetadata` is defined in [`crates/ncm-core/src/metadata.rs`](../crates/ncm-core/src/metadata.rs). Fields we consume:

| JSON field      | Type                              | Notes                                                              |
|-----------------|-----------------------------------|--------------------------------------------------------------------|
| `musicName`     | string                            | Song title                                                         |
| `artist`        | `[[name, id], ...]` nested array  | Deserialized via a custom `deserialize_artists` to flatten names   |
| `album`         | string                            | Album name                                                         |
| `format`        | string                            | `"mp3"` / `"flac"` / `"m4a"` (treated as a hint — see §4)          |
| `bitrate`       | number (bits/sec)                 | Optional. Divide by 1000 for kbps display.                         |
| `duration`      | number (ms)                       | Optional. Format as `mm:ss`.                                       |
| `albumPic`      | string (URL)                      | Optional. Not fetched.                                             |

Implemented as [`parser::read_metadata`](../crates/ncm-core/src/parser.rs).

### 3.3. Audio stream — NCM's custom stream cipher

Here's where NCM deviates from textbook crypto and gets interesting.

**Key scheduling (KSA) — standard RC4.** Given the key bytes from §3.1, build a 256-byte permutation `S`:

```text
for i in 0..256:     S[i] = i
j = 0
for i in 0..256:
    j = (j + S[i] + key[i mod key.len]) mod 256
    swap(S[i], S[j])
```

**Byte generation (PRGA) — *not* standard RC4.** For each ciphertext byte at absolute file offset `off` (0-based within the audio segment), compute:

```text
j  = (off + 1) mod 256
a  = S[j]
b  = S[(a + j) mod 256]
ks = S[(a + b) mod 256]
plaintext_byte = ciphertext_byte XOR ks
```

Notice what's different from standard RC4:

- **No evolving state.** Standard RC4's PRGA maintains two counters `(i, j)` that advance as you read bytes. Here, the key-stream byte at offset `off` depends only on `off` and `S` — it doesn't matter what came before it.
- **Direct offset addressing.** This is what lets the decoder process audio in arbitrary-sized chunks at arbitrary file offsets. No need to buffer the whole song into memory. We use 32 KB chunks ([`format::STREAM_CHUNK_SIZE`](../crates/ncm-core/src/format.rs)).

Implemented as [`crypto::NcmStreamCipher`](../crates/ncm-core/src/crypto/rc4.rs). The `apply(&self, buf: &mut [u8], offset: usize)` method encrypts/decrypts in place — encryption and decryption are the same operation (XOR is self-inverse).

---

## 4. Audio format detection

The metadata JSON has a `format` field, but it can be stale (a file's internal audio might be FLAC even though the hint says "mp3"). So we don't trust it. Instead, after decrypting the first 16 audio bytes, we sniff magic bytes:

| Magic                         | Inferred format |
|-------------------------------|------------------|
| `ID3` or `0xFF 0xFB` / `0xFFFA` | MP3              |
| `fLaC`                        | FLAC             |
| `ftyp` at offset 4            | M4A / AAC        |
| `RIFF...WAVE`                 | WAV              |
| `OggS`                        | Ogg Vorbis       |

If detection succeeds, that wins. If detection is inconclusive, we fall back to the metadata hint. The two-level check is [`AudioFormat::detect`](../crates/ncm-core/src/format.rs) + [`NcmHeaders::effective_format`](../crates/ncm-core/src/decoder.rs).

`info` mode shows both when they disagree (e.g. `FLAC (declared: MP3)`), which is useful for spotting anomalous files.

---

## 5. Module layout

### `ncm-core` (library)

```text
src/
├── lib.rs        public re-exports + crate-level doc
├── error.rs      NcmError (thiserror-derived)
├── format.rs     all magic constants, AudioFormat, CoverMime, sniffing
├── metadata.rs   RawMetadata (serde) + NcmMetadata (user-facing)
├── parser.rs     read_and_verify_magic / read_rc4_key / read_metadata /
│                 skip_crc_gap / read_cover  (one function per segment)
├── decoder.rs    NcmDecoder + NcmHeaders — orchestrates parsing and
│                 head-sniffs the audio format before streaming
└── crypto/
    ├── mod.rs
    ├── aes.rs    aes128_ecb_decrypt wrapper (uses aes + ecb crates)
    └── rc4.rs    NcmStreamCipher — KSA + custom PRGA from §3.3
```

**Dependency direction:** `decoder` calls `parser`, `parser` calls `crypto` + `format` + `metadata`. No cycles. No CLI dependencies — this crate is strictly about bytes.

### `ncm-cli` (binary)

```text
src/
├── main.rs       locale detection, clap dispatch, summary
├── cli.rs        CliCommand enum + clap Command builder + ParsedArgs
├── i18n.rs       Strings struct, EN/ZH tables, Lang::detect_from_env, t()
├── info.rs       `info` subcommand (read-only inspection)
├── pipeline.rs   decrypt pipeline (collect_inputs + process_one + rayon)
├── template.rs   filename template rendering (strfmt + sanitization)
└── tagger.rs     lofty-based tag + cover writer
```

The **dispatch model** in `main.rs`:

```text
prescan_lang(argv)  ──▶  init i18n globals
        │
        ▼
build_command(strings).get_matches()  ──▶  CliCommand enum
        │
        ├──▶ CliCommand::Info        ──▶ info::run
        ├──▶ CliCommand::Completion  ──▶ clap_complete::generate
        └──▶ CliCommand::Decrypt     ──▶ pipeline::run
```

The **pipeline model** in `pipeline.rs`:

```text
collect_inputs(input, recursive)   // single file or walkdir
        │
        ▼
rayon::ThreadPoolBuilder::num_threads(jobs)
        │
        ▼
files.par_iter().for_each(|f| process_one(f, args))
        │         - open NcmDecoder
        │         - filter by --format
        │         - build output path (template or input stem; +folder?)
        │         - conflict check
        │         - stream decrypt to writer
        │         - write tags via lofty
        │         - if --folder: drop cover.jpg/.png next to audio
        ▼
colored status line per outcome (✓ / · / ✗ / [dry-run])
        ▼
RunSummary { ok, skipped, failed } → printed by main.rs
```

Each file is independent; there's no shared decoder state across threads (every `process_one` owns its `NcmDecoder`).

---

## 6. i18n

All user-visible English / Chinese text lives in [`crates/ncm-cli/src/i18n.rs`](../crates/ncm-cli/src/i18n.rs):

- `Strings` struct holds every localized field as `&'static str`.
- `const EN: Strings` and `const ZH: Strings` define translations.
- `t()` is a process-wide accessor; `i18n::init(lang.strings())` is called once from `main` after locale detection.
- Runtime messages use `str::replace("{path}", value)` style placeholders.

When adding a new user-visible string, edit **both** `EN` and `ZH` constants. The type system flags missing fields at compile time, but translation quality is on you.

Clap help text has to be known at `Command::build` time, so `main.rs` does a **pre-scan of argv** for `--lang`/`-L` before building the clap command — that lets `--help` itself be localized.

---

## 7. Parallelism and safety

- Rayon is used only for the outer loop over files. Inside `process_one`, everything is single-threaded.
- `NcmStreamCipher` is `Send + Sync` because it only reads from its `S` box after construction — safe to share across threads in theory, but we actually construct one per file (cheap, ~1 KB of state).
- The `OnceLock<&'static Strings>` in `i18n` is the only shared mutable state, and it's written exactly once at startup.

---

## 8. What we deliberately don't do

- **No transcoding.** We never re-encode audio. If the NCM contained a FLAC, we write out a bit-identical FLAC.
- **No network.** Not a single HTTP call. No account login, no cloud check-in, nothing.
- **No side-channel tricks.** The decryption keys are public constants; we document them in source (see `format.rs`) rather than obfuscating them.
- **No validation of the CRC32 field.** It exists in the file layout but its computation isn't documented, and skipping it has never caused problems on real files.

---

## 9. References

Format knowledge is a collective reverse-engineering effort over the years. The Rust implementation here was informed by:

- [anonymous5l/ncmdump](https://github.com/anonymous5l/ncmdump) — C++ reference, the original
- [nondanee/ncmdump](https://github.com/nondanee/ncmdump) — C, minimal
- [taurusxin/ncmdump](https://github.com/taurusxin/ncmdump) — Go
- [iqiziqi/ncmdump.rs](https://github.com/iqiziqi/ncmdump.rs) — Rust, closest prior art

If you're writing a new parser in a different language and referencing this document, please also cross-check against the above — reverse-engineered format docs can have subtle errors, and triangulating across implementations is the only way to be sure.
