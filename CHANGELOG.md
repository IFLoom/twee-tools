# Changelog

Every change worth knowing about, newest first. The format follows [Keep a Changelog](https://keepachangelog.com),
and twee-tools follows [semantic versioning](https://semver.org) as the README's "Versioning" section describes.
Each change goes under **Unreleased** as it's made; `scripts/release.sh` turns that into the next version.

## [Unreleased]

## [0.1.0] - 2026-11-26

The first release.

### Added

- `build`: Twee to a page to play, with SugarCube 2.37.3, Harlowe 3.3.9, Chapbook 2.3.1 and Snowman 2.1.4 built in,
  story formats found where Tweego looks, and a project's own (`format-paths`); archives, Twine 2 JSON, a `<head>`
  file and modules; `--check-only` to check without building.
- `check`: a whole-story checker for published pages, with rules for SugarCube, Harlowe, Chapbook and Snowman, run by
  every build too; findings with file, line and "did you mean"; `--json` reports.
- `dev`: build, serve and reload on every save, showing errors over the last good build.
- `compress` and `build --compress-to`: a release copy with images re-encoded as WebP (quality, lossless, effort).
- `init`, `decompile`, `formats`, and `formats vendor` to keep a story format's exact version in a project.
- `twee.toml`: sources, output, a pinned format, `public/` and assets, excludes, `[check]`, `[dev]`, `[compress]`,
  `requires`, `trim`.
- Install scripts for Linux, macOS and Windows, and `twee-tools upgrade`.
