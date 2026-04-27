# frozen_string_literal: true

# ncm2mp3 Homebrew formula.
#
# Installing via the raw URL:
#   brew install --formula https://raw.githubusercontent.com/Johnserf-Seed/ncm2mp3/main/dist/homebrew/ncm2mp3.rb
#
# When you tag a new version, the SHA256 strings below need to be refreshed
# with the hashes of the new release archives. The Release workflow emits
# a `.sha256` sibling next to each archive (since v0.4+). On macOS, grab
# them like:
#
#   for target in aarch64-apple-darwin x86_64-apple-darwin; do
#     url="https://github.com/Johnserf-Seed/ncm2mp3/releases/download/vX.Y.Z/ncm2mp3-vX.Y.Z-$target.tar.gz.sha256"
#     echo "$target: $(curl -s "$url" | awk '{print $1}')"
#   done
#
# then paste the two hashes into the two `sha256` lines below.
class Ncm2mp3 < Formula
  desc "Decrypt Netease Cloud Music NCM files to MP3 / FLAC / M4A (Rust CLI)"
  homepage "https://github.com/Johnserf-Seed/ncm2mp3"
  license "Apache-2.0"
  version "0.3.1"

  on_macos do
    on_arm do
      url "https://github.com/Johnserf-Seed/ncm2mp3/releases/download/v#{version}/ncm2mp3-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "cefdefb77dc6c1ad112ff032413efff5fef734c85799515615ae7ae647ce942e"
    end
    on_intel do
      url "https://github.com/Johnserf-Seed/ncm2mp3/releases/download/v#{version}/ncm2mp3-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "e03e0df318bb4f1756f5b95a10025a58b332f4a2c281712cfbd752e4aca3ce79"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Johnserf-Seed/ncm2mp3/releases/download/v#{version}/ncm2mp3-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a6a9a5a47c0c03741ea94ce7efe39a4ae4b42d255fa4b6b9d16116b28101f3da"
    end
    on_intel do
      url "https://github.com/Johnserf-Seed/ncm2mp3/releases/download/v#{version}/ncm2mp3-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f1ef2d425a684a500e69d488109818dd0caccdd634342e155714c30c97173020"
    end
  end

  def install
    # The release tarball unpacks to `ncm2mp3-vX.Y.Z-<target>/` with the
    # binary + README + LICENSE inside. Install the binary and docs.
    bin.install "ncm2mp3"
    doc.install "README.md", "README_en.md", "LICENSE"
  end

  test do
    assert_match "ncm2mp3 #{version}", shell_output("#{bin}/ncm2mp3 --version")
  end
end
