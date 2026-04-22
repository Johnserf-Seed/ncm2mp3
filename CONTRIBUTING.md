# Contributing to ncm2mp3

Thanks for your interest in improving this project.

## Ground rules

- This tool is for decrypting music **you legally own**. Please don't open issues or PRs that facilitate piracy or bulk-scraping of third-party services.
- Be kind. Assume good intent. Technical disagreements are expected; personal attacks are not.

## Prerequisites

- [Rust toolchain](https://rustup.rs/), 1.75 or newer
- A working C linker (Rust's default toolchain handles this on most platforms)

## Build & test

```bash
# clone your fork and enter the repo
git clone https://github.com/<your-username>/ncm2mp3
cd ncm2mp3

# dev build
cargo build

# run all tests (ncm-core unit + integration, ncm-cli unit)
cargo test --workspace

# the checks CI will run on your PR
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

If any of the checks above fail locally, the CI will block your PR, so fix them before pushing.

## Project layout

- `crates/ncm-core/` — pure decryption library (no CLI dependencies). If your change is algorithm-level, it likely lives here.
- `crates/ncm-cli/` — the binary `ncm2mp3`. CLI parsing, i18n, pipeline, file I/O.
- `docs/ARCHITECTURE.md` — format and code structure reference.
- `.github/workflows/` — CI and release automation.

## Making a change

1. Open an issue first for non-trivial work, so we can agree on direction before code is written.
2. Fork, branch from `main`.
3. Keep commits focused and atomic; one logical change per commit.
4. Write tests. For `ncm-core`, prefer unit tests in the module plus a roundtrip test in `crates/ncm-core/tests/roundtrip.rs` if the change touches the wire format. For `ncm-cli`, unit tests near the code where possible.
5. If you change user-visible strings, update **both** English (`EN`) and Chinese (`ZH`) entries in `crates/ncm-cli/src/i18n.rs`.
6. If you add a public `ncm-core` API, document it with a `///` doc comment. The crate has `#![warn(missing_docs)]` on (once A2 lands), and CI treats warnings as errors.
7. Run the full check set locally (`fmt`, `clippy`, `test`, `build --release`).
8. Open a PR against `main`. Fill in the PR template; explain what changed and how it was verified.

## Commit message style

Loosely follow [Conventional Commits](https://www.conventionalcommits.org/). Typical prefixes:

- `feat:` / `feat(cli):` / `feat(core):` — new user-visible capability
- `fix:` — bug fix
- `refactor:` — non-behavioral code change
- `docs:` — documentation only
- `ci:` — workflows / release automation
- `chore(deps):` — dependency bumps
- `test:` — tests only

Subject under 72 chars, imperative mood ("add", not "adds" or "added"). Body explains *why*, not *what* (the diff shows the what).

## NCM format questions

If you're fixing a parser bug or adding a new NCM variant, read `docs/ARCHITECTURE.md` first — it documents the byte-level format, the XOR masks, the AES keys, and the quirky RC4-variant stream cipher. Cross-reference with real reference implementations (see README credits section) rather than guessing.

## Reporting security issues

Don't file public GitHub issues for vulnerabilities. See [SECURITY.md](./SECURITY.md).

## License

By submitting a contribution, you agree that it may be distributed under the project's [Apache-2.0](./LICENSE) license.
