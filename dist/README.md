# Distribution manifests

Package-manager manifests for one-command installation.

| Target | File | Installation command |
|---|---|---|
| Scoop (Windows) | [`scoop/ncm2mp3.json`](./scoop/ncm2mp3.json) | `scoop install https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/scoop/ncm2mp3.json` |
| Homebrew (macOS + Linux) | [`homebrew/ncm2mp3.rb`](./homebrew/ncm2mp3.rb) | `brew install --formula https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/homebrew/ncm2mp3.rb` |

Both point at prebuilt binaries on the [Releases page](https://github.com/Johnserf-Seed/ncm2mp3/releases).

## Bumping the version

When you tag a new `vX.Y.Z`:

1. **Wait** for the Release workflow to finish uploading archives + `.sha256` files.
2. **Update Scoop manifest** (`scoop/ncm2mp3.json`):
   - Change `version` to `X.Y.Z`.
   - Update the two `url` entries under `architecture` to point at the new release.
   - Update the two `hash` entries. Get the hashes from the `.sha256` siblings on the Releases page:
     ```bash
     for target in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
       url="https://github.com/Johnserf-Seed/ncm2mp3/releases/download/vX.Y.Z/ncm2mp3-vX.Y.Z-${target}.zip.sha256"
       echo "${target}: $(curl -sL "$url" | awk '{print $1}')"
     done
     ```
   - Update the `bin` path if the archive layout changes (unlikely).

3. **Update Homebrew formula** (`homebrew/ncm2mp3.rb`):
   - Change the `version "X.Y.Z"` line.
   - Fetch the 4 macOS/Linux archive SHA256s (same pattern as above, but
     with `aarch64-apple-darwin`, `x86_64-apple-darwin`,
     `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu` targets
     and `.tar.gz.sha256` extension).
   - Paste the four hashes into the four `sha256` lines.

4. Commit + push. No tag needed for manifest updates.

## Why not a dedicated tap / bucket repo?

For a project this size, shipping the manifests inside the main repo and pointing at them via `raw.githubusercontent.com` is simpler than maintaining a separate `homebrew-tap` or `scoop-bucket` repo. If ncm2mp3 ever outgrows that (e.g. gets accepted into `homebrew-core` or Scoop's `extras` bucket), that's a future refactor.
