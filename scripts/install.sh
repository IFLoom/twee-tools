#!/bin/sh
# Installs twee-tools on Linux or macOS, or upgrades it: downloads the release for this system, checks it against the
# release's checksums, and puts `twee-tools` in ~/.local/bin.
#
#   curl -fsSL https://github.com/IFLoom/twee-tools/releases/latest/download/install.sh | sh
#
# Settings, as environment variables:
#   TWEE_TOOLS_VERSION       a version to install, such as 0.2.0 (default: the latest)
#   TWEE_TOOLS_INSTALL_DIR   where to put it (default: ~/.local/bin)
#   TWEE_TOOLS_DOWNLOAD_URL  where the release's files are (default: the GitHub release)
set -eu

repo="https://github.com/IFLoom/twee-tools"
version="${TWEE_TOOLS_VERSION:-}"
version="${version#v}"
dir="${TWEE_TOOLS_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
  echo "twee-tools install: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Linux) os=unknown-linux-musl ;;
  Darwin) os=apple-darwin ;;
  *) fail "this script is for Linux and macOS; on Windows, run install.ps1 in PowerShell" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) fail "there's no build for $(uname -m) processors" ;;
esac
# A Mac with Apple silicon reports x86_64 when the shell runs under Rosetta: install the native build.
if [ "$os" = apple-darwin ] && [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
  arch=aarch64
fi
target="$arch-$os"

if [ -n "${TWEE_TOOLS_DOWNLOAD_URL:-}" ]; then
  base="${TWEE_TOOLS_DOWNLOAD_URL%/}"
elif [ -n "$version" ]; then
  base="$repo/releases/download/v$version"
else
  base="$repo/releases/latest/download"
fi

if command -v curl >/dev/null 2>&1; then
  fetch() { curl --proto '=https,http' -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q "$1" -O "$2"; }
else
  fail "it needs curl or wget to download"
fi
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
archive="twee-tools-$target.tar.gz"

echo "Downloading twee-tools ${version:-(the latest)} for $target..."
fetch "$base/$archive" "$tmp/$archive" || fail "couldn't download $base/$archive"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "couldn't download $base/SHA256SUMS"
expected=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || fail "the release's SHA256SUMS doesn't list $archive"
[ "$(sha256 "$tmp/$archive")" = "$expected" ] || fail "$archive doesn't match its checksum: the download is damaged or not the release's"

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$dir"
# Into place through a new file and a rename, so a running twee-tools is never left half-written.
cp "$tmp/twee-tools-$target/twee-tools" "$dir/.twee-tools.new"
chmod 755 "$dir/.twee-tools.new"
mv -f "$dir/.twee-tools.new" "$dir/twee-tools"

echo "Installed $("$dir/twee-tools" --version) in $dir."
case ":$PATH:" in
  *":$dir:"*) ;;
  *)
    echo
    echo "$dir isn't on your PATH yet. Add this line to your shell's profile (~/.bashrc, ~/.zshrc), then open a new terminal:"
    echo "  export PATH=\"$dir:\$PATH\""
    ;;
esac
echo "Upgrade later with: twee-tools upgrade"
