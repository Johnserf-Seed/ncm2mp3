# Security Policy

## Supported versions

Only the latest release on the `main` branch is supported with security fixes. We do not backport to older tags.

## Reporting a vulnerability

**Please do not open a public GitHub issue for security-sensitive problems.**

Instead, use GitHub's private reporting channel:

➡️ <https://github.com/Johnserf-Seed/ncm2mp3/security/advisories/new>

That opens a private conversation visible only to repository maintainers.

### What to include

- A clear description of the issue and its impact.
- Minimal reproduction steps — ideally a small `.ncm` test file (or a way to generate one) and the exact command that triggers the bug.
- The affected version(s) — commit hash or release tag.
- Your suggested severity, if you have an opinion.

### What to expect

- Acknowledgement within **5 working days**.
- A fix or a rejection rationale within **30 days** for issues confirmed valid.
- Credit in the release notes and CHANGELOG, unless you prefer to stay anonymous.

## Scope

This project is a local decryption tool. In-scope concerns include:

- Crafted `.ncm` files that cause the process to crash unexpectedly, hang, or consume unbounded memory / disk.
- Path traversal or similar file-write misbehavior when processing attacker-controlled filenames or metadata fields.
- Buffer-handling bugs in the `ncm-core` decryption primitives.

Out of scope:

- Legality or copyright status of decrypted audio files — that's on the user, not the tool.
- Third-party services (Netease Cloud Music itself, their servers, their APIs). We don't talk to any network endpoint.
