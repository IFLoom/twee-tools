#!/bin/sh
# Prepares a release: sets the version, turns the changelog's Unreleased section into that version, checks, commits and
# tags. It pushes nothing; it prints the push that publishes the release (the release workflow builds it from the tag).
#
#   scripts/release.sh 0.2.0
set -eu

version="${1:-}"
case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "Give the version to release, x.y.z: scripts/release.sh 0.2.0" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/.."

if [ -n "$(git status --porcelain)" ]; then
  echo "Commit or stash your changes first: a release is made from a clean checkout." >&2
  exit 1
fi
if [ "$(git branch --show-current)" != "main" ]; then
  echo "Release from main." >&2
  exit 1
fi
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
  echo "v$version is already tagged." >&2
  exit 1
fi
# The Unreleased section must say what's in the release.
unreleased=$(awk '/^## \[Unreleased\]/{on=1; next} /^## \[/{on=0} on' CHANGELOG.md | grep -c '[^[:space:]]' || true)
if [ "$unreleased" -eq 0 ]; then
  echo "CHANGELOG.md's Unreleased section is empty: say what's in $version first." >&2
  exit 1
fi

# The version, in the one place it lives, and the lock file that repeats it.
sed -i.bak "s/^version = \"[^\"]*\"/version = \"$version\"/" Cargo.toml && rm Cargo.toml.bak
cargo update --workspace --offline -q
# The changelog: a fresh Unreleased section above the new version's.
today=$(date -u +%Y-%m-%d)
awk -v v="$version" -v d="$today" '{ print } /^## \[Unreleased\]$/ { print ""; print "## [" v "] - " d }' CHANGELOG.md >CHANGELOG.md.new
mv CHANGELOG.md.new CHANGELOG.md

cargo test -q --locked >/dev/null
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -q -m "Release $version"
git tag -a "v$version" -m "twee-tools $version"
echo "Made v$version. Publish it with:"
echo "  git push origin main v$version"
