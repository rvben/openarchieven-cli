#!/usr/bin/env bash
#
# Render the Homebrew formula for openarchieven.
#
# Reads SHA256 sidecar files in dist/ produced by `make release-archive`
# and prints a complete Formula/openarchieven.rb to stdout.
#
# Usage:
#   scripts/render-homebrew-formula.sh <version> <tag>
#
# e.g.
#   scripts/render-homebrew-formula.sh 0.1.0 v0.1.0

set -euo pipefail

if [ $# -ne 2 ]; then
  echo "usage: $0 <version> <tag>" >&2
  exit 2
fi

version="$1"
tag="$2"

read_sha() {
  local target="$1"
  local file="dist/openarchieven-${version}-${target}.tar.gz.sha256"
  if [ ! -f "$file" ]; then
    echo "missing sha256 file: $file" >&2
    exit 1
  fi
  awk '{print $1}' "$file"
}

sha_aarch64_apple_darwin=$(read_sha aarch64-apple-darwin)
sha_x86_64_apple_darwin=$(read_sha x86_64-apple-darwin)
sha_aarch64_unknown_linux_gnu=$(read_sha aarch64-unknown-linux-gnu)
sha_x86_64_unknown_linux_gnu=$(read_sha x86_64-unknown-linux-gnu)

cat <<FORMULA
class Openarchieven < Formula
  desc "Command-line interface to the Open Archives genealogical API"
  homepage "https://github.com/rvben/openarchieven"
  version "${version}"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/rvben/openarchieven/releases/download/${tag}/openarchieven-${version}-aarch64-apple-darwin.tar.gz"
      sha256 "${sha_aarch64_apple_darwin}"
    else
      url "https://github.com/rvben/openarchieven/releases/download/${tag}/openarchieven-${version}-x86_64-apple-darwin.tar.gz"
      sha256 "${sha_x86_64_apple_darwin}"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/rvben/openarchieven/releases/download/${tag}/openarchieven-${version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${sha_aarch64_unknown_linux_gnu}"
    else
      url "https://github.com/rvben/openarchieven/releases/download/${tag}/openarchieven-${version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${sha_x86_64_unknown_linux_gnu}"
    end
  end

  def install
    bin.install "openarchieven"
  end

  test do
    system "#{bin}/openarchieven", "version"
  end
end
FORMULA
