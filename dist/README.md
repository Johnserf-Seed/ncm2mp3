# Distribution manifests

Package-manager manifests for one-command installation.

| Target | File | Installation command |
|---|---|---|
| Scoop (Windows) | [`scoop/ncm2mp3.json`](./scoop/ncm2mp3.json) | `scoop install https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/scoop/ncm2mp3.json` |
| Homebrew (macOS + Linux) | [`homebrew/ncm2mp3.rb`](./homebrew/ncm2mp3.rb) | `brew install --formula https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/homebrew/ncm2mp3.rb` |

Both point at prebuilt binaries on the [Releases page](https://github.com/Johnserf-Seed/ncm2mp3/releases).

## Bumping the version

**Automatic** — do nothing. Since v0.3.0 the
[`.github/workflows/update-dist.yml`](../.github/workflows/update-dist.yml)
workflow fires when a Release is published, fetches the 6 SHA256 hashes
from the release's `.sha256` sibling files, splices them + the new
version into both manifests, and opens a PR (branch `dist-sync/vX.Y.Z`)
for you to review and merge.

To trigger manually (e.g. to backfill an old tag or retry after a fix):

```bash
gh workflow run update-dist.yml -f tag=vX.Y.Z
```

### If you really need to do it by hand

1. Wait for Release to finish uploading archives + `.sha256` files.
2. Grab the 6 hashes:
   ```bash
   for t in \
     x86_64-pc-windows-msvc aarch64-pc-windows-msvc \
     x86_64-apple-darwin aarch64-apple-darwin \
     x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
   do
     ext=zip; [[ "$t" == *linux* || "$t" == *darwin* ]] && ext=tar.gz
     printf "%-35s" "$t:"
     curl -sL "https://github.com/Johnserf-Seed/ncm2mp3/releases/download/vX.Y.Z/ncm2mp3-vX.Y.Z-${t}.${ext}.sha256" | awk '{print $1}'
   done
   ```
3. Paste: 2 hashes (Windows) into `scoop/ncm2mp3.json`, 4 hashes (macOS+Linux) into `homebrew/ncm2mp3.rb`; bump `version` in both. Commit + push.

## Why not a dedicated tap / bucket repo?

For a project this size, shipping the manifests inside the main repo and pointing at them via `raw.githubusercontent.com` is simpler than maintaining a separate `homebrew-tap` or `scoop-bucket` repo. If ncm2mp3 ever outgrows that (e.g. gets accepted into `homebrew-core` or Scoop's `extras` bucket), that's a future refactor.
