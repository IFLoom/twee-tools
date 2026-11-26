# twee-tools

[![CI](https://github.com/IFLoom/twee-tools/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/IFLoom/twee-tools/actions/workflows/ci.yml)
[![Release](https://github.com/IFLoom/twee-tools/actions/workflows/release.yml/badge.svg)](https://github.com/IFLoom/twee-tools/releases/latest)
[![Licence: MIT or Apache-2.0](https://img.shields.io/badge/licence-MIT%20or%20Apache--2.0-blue)](#licences)

A Twee compiler, story checker and media compressor for [Twine](https://twinery.org) stories, in one program.

- **Build** Twee into a page to play, as [Tweego](https://www.motoslave.net/tweego/) does, with SugarCube, Harlowe,
  Chapbook and Snowman built in, so there's nothing else to install.
- **Check** the whole story every time you build: broken links, passages nothing reaches, dead ends, misspelt macros
  and variables, JavaScript that doesn't parse, missing media, each with its file and line. Errors stop the build
  before a player meets them.
- **Shrink** a story's images to WebP for release, and rewrite the story to point at them.

It checks published pages too, so it works for stories made in the Twine editor as well as Twee projects.

> twee-tools is at 0.1: before 1.0, a minor version may still change its options (see [Versioning](#versioning)).
> Its page is [ifloom.dev/twee-tools](https://ifloom.dev/twee-tools).

```
$ twee-tools build --check-only
 hstory  Harlowe 3.3.9
 12 passages · 20 words · 12 links · 19 macros · 3 variables · 0 media files

 ERRORS
 ✖ Broken links · 2
   Cellar  story.twee:27
     The macro goes to "Nowehre", and there's no passage by that name
   Menu  story.twee:45
     The macro goes to "Mapp", and there's no passage by that name
 ✖ Unknown macros and inserts · 1
   Typo  story.twee:51
     (lnk-goto:) isn't a Harlowe macro
     → did you mean (link-goto:)?
 ✖ Links, macros and hooks left open · 1
   Typo  story.twee:51
     A hook opened with [ is never closed with ]

 WARNINGS
 ⚠ Variables read, and never set · 1
   $missing  in Vault · story.twee:30
```

## Contents

- [Installing](#installing)
- [Quick start](#quick-start)
- [Commands](#commands)
- [Configuration: twee.toml](#configuration-tweetoml)
- [What the checks find](#what-the-checks-find)
- [Compression](#compression)
- [Moving from Tweego](#moving-from-tweego)
- [Speed](#speed)
- [Development](#development)
- [Licences](#licences)

## Installing

twee-tools is one program with nothing else to install, so install it once, globally, as you would Tweego. Each project
names the story format it's written for (`format = "SugarCube@2.37.3"` in `twee.toml`, which `init` writes for you),
and `twee-tools formats vendor` keeps that exact version in the project, so a newer twee-tools still builds the same
page.

- **Linux and macOS:**

  ```sh
  curl -fsSL https://github.com/IFLoom/twee-tools/releases/latest/download/install.sh | sh
  ```

  It installs to `~/.local/bin` (`TWEE_TOOLS_INSTALL_DIR` changes that) and says if that folder isn't on your PATH.
- **Windows,** in PowerShell:

  ```powershell
  irm https://github.com/IFLoom/twee-tools/releases/latest/download/install.ps1 | iex
  ```

  It installs to `%LOCALAPPDATA%\Programs\twee-tools` and adds that folder to your PATH.
- **By hand:** each [release](https://github.com/IFLoom/twee-tools/releases) has an archive for each system (Linux
  x86-64 and ARM, macOS Apple silicon and Intel, Windows x64), with `SHA256SUMS` to check them against. The Linux
  builds are static, so they run on any distribution.
- **With Rust installed:** `cargo install --git https://github.com/IFLoom/twee-tools twee-tools` builds and installs
  it from the repository, or, in a checkout, `cargo install --path crates/cli`.
- **npm** (planned, if asked for): `npm i -D twee-tools`, one binary per platform as esbuild ships, for JavaScript
  projects that want the version in their lock file.

Both scripts download the release for your system and check it against the release's checksums before installing.
`TWEE_TOOLS_VERSION=0.2.0` installs one particular version, newer or older.

**Upgrading:** `twee-tools upgrade` installs the latest release in place of the running one, checked the same way
(`--version 0.2.0` for one particular version, `--check` to only say whether there's a newer one). If twee-tools came
from Homebrew, Scoop or `cargo install`, it says to upgrade with those instead. twee-tools goes online only when you
run `upgrade`.

## Quick start

A new project:

```sh
twee-tools init my-story --format sugarcube   # or harlowe, chapbook, snowman
cd my-story
twee-tools dev --open                         # serve on http://127.0.0.1:8080; every save rebuilds and reloads
```

`init` makes `twee.toml`; `src/` with a story (and a fresh IFID), its JavaScript and its stylesheet; and `public/`, for
images and anything else that ships beside the page as it is. `dev` keeps
serving the last good build when one fails, and shows the errors over the page until you fix them.

Then:

```sh
twee-tools build                                 # check, then write dist/index.html
twee-tools build --check-only --strict           # check only, as in CI: exit 1 on errors or warnings
twee-tools check story.html                      # check a published page, from the Twine editor or anyone's
twee-tools compress story.html --out release/    # a copy with its images as WebP
twee-tools decompile story.html --split src/     # a published story back to Twee, to keep in git
```

## Commands

| Command | What it does |
|---|---|
| `init [dir] [--format f]` | Make a project: `twee.toml`, `src/` with a story, its JavaScript and stylesheet, and `public/`. |
| `dev` | Build, serve and watch: every save rebuilds and reloads the browser. Errors show over the last good build. |
| `build [paths]` | Check the Twee sources, then write the page (or `--archive`, `--json-output`). |
| `build --check-only` | Check exactly as a build would, show the whole report, and write nothing. |
| `check <page>` | Check a published `.html` story, or a folder holding one. |
| `compress <story> --out <dir>` | Write a copy of a published story with its images re-encoded as WebP. |
| `decompile <page>` | Turn a published story (or a Twine archive) back into Twee: one file, or `--split` into many. |
| `formats [list \| path \| licenses]` | The story formats: built in, installed where Tweego looks, and the project's own. |
| `formats vendor [format]` | Copy a story format into the project's `formats/`, so its builds keep that exact version. |
| `upgrade [--version x.y.z] [--check]` | Install the latest twee-tools (or that version) in place of this one. |

Every command takes `-C <dir>` to run as if started in another folder, and `twee-tools <command> --help` lists the
rest of its options.

**Exit codes:** 0 when all is well; 1 when the checks fail or a build breaks; 2 for a mistake in how it was run.

### `check` or `build`?

`check` reads a published page; `build` reads Twee. A page already holds exactly what ships, so `check` needs nothing
else. Twee needs the project's rules (what to leave out, which fonts and media go in, where the page will be), so only
`build` reads it, and checks it on the way.

| | `check` | `build` | `build --check-only` |
|---|---|---|---|
| Reads | a published page, or a folder holding one | Twee: a project, or Twee files and folders | the same as `build` |
| Writes | nothing | the page (and, if asked, a release copy) | nothing |
| Report | everything, notes included | errors and warnings | everything, notes included |
| Fails when | errors, or warnings with `--strict` | the same, and then it writes nothing | the same |

The full reports take `--json` (a report to keep), `--errors-only` and `--all` (every entry of a long list). Every
report opens with the story's counts: its passages, the words a player reads (prose, without code or markup, so not
Tweego's count of characters over five), links, macros, variables and media files. The page a
build writes can be checked too: `twee-tools check dist/index.html` finds what the sources' check finds.

**In CI:** a Twee project runs `twee-tools build --check-only --strict`; a published page runs
`twee-tools check story.html --strict`.

### Building

- **Story formats** need no setup: SugarCube 2.37.3, Harlowe 3.3.9, Chapbook 2.3.1 and Snowman 2.1.4 are built in.
  Others are found where Tweego looks (`storyformats`, `.storyformats` or `story-formats` beside the program, in your
  home folder or the working folder, then `TWEEGO_PATH`), and a project's own (its `formats/` folder, or
  `format-paths`) win over all of them; `twee-tools formats vendor` copies a format there. The
  format is the one `--format` or `twee.toml` names (`sugarcube`, `SugarCube@2.37.3`, `harlowe-3`), else the story's
  own, chosen as Tweego chooses: the same major version, the greatest at or above the one asked for.
- **Sources:** `.twee`, `.tw` (and Twee2's `.twee2`, `.tw2`) files and folders of them, in name order, read as UTF-8
  (a byte-order mark is fine), or as Windows-1252, with a warning, when a file isn't UTF-8, as Tweego reads them. Each
  passage is trimmed of spaces and blank lines at both ends, as Tweego trims it; `--no-trim` (or `trim = false`) keeps
  them, for a story where they matter. `.js` and `.css`
  files become the story's JavaScript and stylesheet, and fonts become `@font-face` stylesheets. With `--embed-media`,
  images, audio, video and subtitles become Twine media passages, as Tweego always makes them; without it they stay
  files beside the page.
- **The page** is written as Twine writes it: its attribute order and escaping, tag colours sorted, a `</script>` in
  your JavaScript escaped so it can't end the element, and default passage positions as Twine places them. A passage
  declared twice keeps its last declaration, in the first one's place, as story formats play it. The same sources
  always give the same bytes.

## Configuration: twee.toml

`build`, `dev` and `check` read the nearest `twee.toml`: in the working folder, or the first folder above it that has
one. Paths in it are relative to it, and options on the command line win over it. Every key is optional.

```toml
sources = ["src"]                  # default: src
output = "dist/index.html"         # default: dist/index.html
format = "SugarCube@2.37.3"        # at least this version, as Tweego reads it (default: the story's own)
start = "Start"
head = "head.html"                 # HTML for the page's <head>
modules = ["modules"]              # JavaScript, CSS and fonts for the <head>, before the story runs
exclude = ["src/drafts/**", "**/*.bak"]   # sources to leave out (also --exclude)
embed-media = false
strict = false                     # stop on warnings too
trim = true                        # trim spaces and blank lines at both ends of each passage, as Tweego does (also --no-trim)
public = "public"                  # copied as it is beside the page; the default when the folder's there
assets = ["../shared/art"]         # more folders copied beside the page, each by its name: dist/art
format-paths = ["formats"]         # the project's own story formats, which win (default: formats/, when it's there)
requires = "0.1"                   # the oldest twee-tools that builds this project

[check]
config = ["macros.twee-config.yml"]   # more checker settings, besides those beside the story
ignore = ["unused-media"]             # finding codes never reported (also --ignore)
severity = { dead-end = "error" }     # a code's severity: error, warning or note

[dev]
port = 8080
open = true
test = true                        # SugarCube's debug views while developing; never for `build`, unless asked

[compress]
enabled = true                     # also write a release copy on every build
output = "release"
quality = 80
lossless = false
effort = 4
max-width = 1920
exclude = ["**/*.psd", "notes/**"] # files beside the page to leave out of the release
```

- **`public/`** holds what ships beside the page as it is: its contents are copied into the output folder as they're
  laid out, so `public/media/sea.png` becomes `dist/media/sea.png`, and a story's `<img src="media/sea.png">` is the
  path under `public/`. It's used whenever the folder exists (`init` makes one); `public = "static"` names another,
  and `public = false` turns it off. A file named like the built page (`public/index.html`) is never copied over it.
- **`assets`** are more folders or files to copy, each by its own name (`../shared/art` becomes `dist/art`), for things
  that live outside the project.
- Both are copied after a build that passes its checks, and a file already there unchanged isn't copied again. Media in
  them are checked where they'll be, so `build --check-only` finds `media/sea.png` without copying anything.
- **Leaving files out:** `exclude` leaves Twee, JavaScript, CSS, fonts and media out of the build (globs relative to
  `twee.toml`, or to the working folder for `--exclude`). `[compress] exclude` leaves files beside the page out of the
  release copy, relative to the page; a pattern with no `/` matches at any depth. `.git`, `node_modules` and `target`
  are never copied, and leaving out a file the story uses is refused, since the release would break.
- **`requires`:** a twee-tools older than this says to upgrade, and stops.

### The checker's settings

Macros and variables that the story's own code adds, which the checker can't see, go in `*.twee-config.yml` files: the
format the Twee 3 Language Tools editor extension reads, so one file serves both. Those beside the story are read; more
can be named with `--config` or `[check] config`.

To quiet a check that doesn't suit a story:

- `[check] ignore` (or `--ignore`) drops a code everywhere, and `[check] severity` raises or lowers one.
- A passage tagged `twee-tools-ignore` has none of its findings reported; one tagged `twee-tools-ignore-dead-end` (any
  code after the dash) loses only that one. Tags work in the Twine editor and in published pages too.

`check` on a page reads the project's `[check]` as well, so `check dist/index.html` agrees with the build.

## What the checks find

Severity: **error** breaks the story, or the format refuses it; **warning** is likely a mistake; **note** still works
and is worth knowing (deprecated, or state never used). Notes never change the exit code.

What applies depends on the story's format, decided once; a story that names none is SugarCube, as `build` builds it.

- **Every format:** the story's data (IFID, start, title, format); passages declared twice, left unwritten, or that
  nothing reaches; broken links; JavaScript that doesn't parse; missing, misnamed and oversized media, and media nothing
  uses. Dead ends, in the four built-in formats.
- **SugarCube 2:** everything in the table below.
- **Harlowe 3:** macros checked against the bundled Harlowe's own list (matched as Harlowe matches names, ignoring case,
  `-` and `_`, with "did you mean"), and those an add-on defines in the story's JavaScript (`Macros.add({…})`, as
  Chapel's Harlowe Audio Library does). Passages its macros go to or show (`go-to`, `redirect`, `display`, `link-goto`,
  `click-goto`, …) count as links. Macro calls and hooks left open, `(else:)` with no hook before it, `$variables` read
  and never set (`set … to`, `put … into` and `bind` all set), and deprecated macros. Links are read as Harlowe's own
  lexer reads them, and storylets, `startup`, `header` and `footer` passages, and passages a story picks by tag count
  as reached.
- **Chapbook 2:** inserts and modifiers checked against the bundled Chapbook's own, and those the story adds
  (`engine.template.inserts.add`). `{link to:}`, `{embed passage:}` and `{reveal link:, passage:}` count as links, as do
  links in the header and footer a vars section sets. `[else]` with no `[if]` before it, and variables shown that
  nothing sets. Links, vars sections and modifiers are read by Chapbook's own rules.
- **Snowman 2:** `story.show`, `story.render`, `story.passage`, `renderToSelector`, `hasVisited` and `visited` count as
  links, and `<% … %>` templates left open are reported.

Where a format's documentation is silent, its rules come from the format's own code: how each format divides and
trims a link, for one, differs between all four. Harlowe's and Chapbook's checks were tested against 31 public stories
in those formats, and every format's against the 170 examples in the Twine Cookbook.

### SugarCube's checks

| Code | Severity | What |
|---|---|---|
| `start`, `ifid`, `story-title`, `story-format`, `story-data` | error / warning / note | A start passage that exists and isn't code; an IFID that's an uppercase v4 UUID, and not the spec's example; a title; a format Twine has, with an x.y.z version; StoryData that's JSON with known keys. |
| `format-version` | error | A SugarCube feature newer than the version the story declares (`<<do>>` 2.37, `_args` 2.36, `<<type>>` 2.32, …). |
| `twee-header`, `passage-name`, `html-structure` | warning / error | Twee headers indented, with tags after metadata, or bad metadata; names with link characters; published HTML with unescaped markup, missing pids, or several scripts. |
| `broken-link` | error | A passage that isn't there, from a link, `<<goto>>`, `<<include>>`, `<<link>>`, `<<back>>`, an image link, `data-passage`, `Story.get()` or `Engine.play()`, with "did you mean". |
| `code-passage` | error | Going to a special or code-tagged passage; `<<widget>>` outside a widget passage. |
| `special-name`, `twine1-tags`, `no-output` | error / warning | `Storyinit`, `NOBR` and other special names written almost right; `script` and `stylesheet` tags in a Twine 2 story; words in StoryInit and other passages that show nothing. |
| `removed-api`, `deprecated` | error / note | Macros and APIs SugarCube 2.37 removed (and `is not`, which no longer means `isnot`), or deprecated, each with its replacement. |
| `config`, `macro-name` | error / warning | `Config.*` settings SugarCube doesn't have, `Config.debug` left on, `State.prng.init()` called late; widgets named after SugarCube's own macros. |
| `js-syntax`, `widget-as-function` | error | JavaScript that doesn't parse (the story's, `<<script>>`, `<<set>>`, `<<if>>`, link setters, `@attr`); a widget called like a function. |
| `if-assignment`, `receiver`, `macro-args`, `switch`, `variable-name`, `capture`, `audio`, `wrong-format` | error | `=` in `<<if>>`; unquoted receivers; expressions in pieces; `<<default>>` not last; `$1st`, `$my-var`; links in `<<for>>` without `<<capture>>`; audio never set up; Harlowe markup in SugarCube, or the other way round. |
| `naked-variable`, `goto-continues`, `link-target`, `def-precedence` | warning / note | `$name.method()` in prose; code after `<<goto>>`; setters on outside links; `def` without parentheses. |
| `unknown-macro`, `unclosed-macro`, `stray-close`, `misplaced-macro`, `unclosed-markup` | error | Macros SugarCube doesn't have, containers not closed, closing tags with nothing open, `<<else>>` outside `<<if>>`, `[[` or `<<` left open. |
| `duplicate-passage` | error | Two passages with one name (the later one is built). |
| `missing-media`, `absolute-media` | error / warning | A path naming no file (with "did you mean"), or only by its capitals; a path from this computer (`C:\…`, `file://`). |
| `unreachable`, `dead-end`, `unwritten` | warning | Passages nothing leads to; reached passages with no way forward; empty passages, or Twine's placeholder text. |
| `never-set`, `never-read` | warning / note | Variables and their properties read but never set, or set but never read. |
| `unused-macro`, `unused-media`, `misnamed-media`, `large-media` | warning | Widgets nothing calls; files nothing uses, whose extension isn't what they are, or of 5 MB or more. |

## Compression

`twee-tools compress story.html --out release/` writes a copy of a published story with its JPEG, PNG and GIF images
(animated ones too) re-encoded as WebP, and the story rewritten to point at them. The originals are never touched.
`build --compress-to release`, or `[compress] enabled = true`, writes the same release copy on every build.

| Option | Default | What |
|---|---|---|
| `quality` | 80 | 0 to 100. Lossy: how close to the original. Lossless: how hard libwebp works on the size. |
| `lossless` | false | Keep every pixel exactly. Often smaller than PNG for flat art; bigger than lossy for photos. |
| `effort` | 4 | 0 (fast) to 6 (smallest): libwebp's method. |
| `max-width`, `max-height` | none | Scale larger images down to fit. |
| `exclude` | none | Files beside the page to leave out of the copy. |

Each is a `[compress]` key in `twee.toml`, or a flag of the same name, which wins. `--dry-run` reports what would
change; `--keep-names` also converts images whose paths the story builds as it runs, keeping their names; and
`--drop-unused` leaves out files nothing uses. A release folder is only ever replaced if twee-tools made it.

## Moving from Tweego

| Tweego | twee-tools |
|---|---|
| `tweego -o out.html src` | `twee-tools build src -o out.html`, or `twee-tools build` with a `twee.toml` |
| `-f sugarcube-2` | `--format sugarcube-2`, or `--format SugarCube@2.37.3` |
| `-s Start`, `-t`, `--head f`, `-m dir` | `--start Start`, `--test`, `--head f`, `--module dir` |
| `-w` | `twee-tools dev`: serves and reloads too, and keeps going after an error |
| `-a`, `-j` | `--archive`, `--json-output` |
| `-d story.html` | `twee-tools decompile story.html` |
| `--list-formats` | `twee-tools formats` |
| story formats installed by hand | built in; installed ones are still found, and a project can carry its own |
| (no checking) | every build checks first; `--check-only` checks without building |
| media in the sources become passages | only with `--embed-media`; otherwise they stay files beside the page |
| `--no-trim` | `--no-trim`, or `trim = false` |
| `--charset` | none needed: UTF-8, with or without a byte-order mark, else Windows-1252, read without asking |
| `--log-stats` | always in the report: passages, words, links, macros, variables and media; `--json` gives them as `stats` |
| Twine 1 output | not supported |

The story data twee-tools writes matches Tweego 2.1.1's byte for byte for the same story and format (tested on a
110-passage project), except the attributes of `<tw-storydata>`, which follow Twine's order and name twee-tools as the
creator.

## Speed

Median build time and peak memory on Linux (under WSL2), building the same SugarCube story with each tool, with checks
off for a like-for-like compile:

| Story | twee-tools | Tweego 2.1.1 | twee-ts 2.1.0 | Extwee 2.4.3 |
|---|---|---|---|---|
| A real project: 110 passages, 15 files | **10 ms**, 15 MB | 35 ms, 12 MB | 265 ms, 162 MB | 63 ms, 71 MB |
| A published game, decompiled: 914 passages, 7 MB | **42 ms**, 47 MB | 182 ms, 75 MB | 1,001 ms, 366 MB | 171 ms, 176 MB |
| 5,000 passages, 50 files | **20 ms**, 27 MB | 175 ms, 24 MB | 503 ms, 187 MB | 156 ms, 96 MB |
| 20,000 passages, 50 files | **52 ms**, 59 MB | 852 ms, 73 MB | 1,274 ms, 331 MB | 1,210 ms, 169 MB |

With its checks on, which no other compiler has, twee-tools checks the 20,000-passage story in 0.17 s and the
914-passage game in 0.23 s: a whole check costs less than Tweego takes to build. `scripts/bench.py` reruns the
benchmark, and `scripts/synthetic.py` makes the synthetic stories.

## Versioning

twee-tools follows [semantic versioning](https://semver.org). Its public surface is what a project or a script
relies on: the command-line options, the `twee.toml` keys, the exit codes, the finding codes, the shape of the
`--json` report (which says its own version, `"report": 1`), and which story-format versions are built in. Before
1.0, a change that breaks any of these raises the minor version (0.1 to 0.2), and anything else the patch (0.1.0 to
0.1.1). Every change is in [CHANGELOG.md](CHANGELOG.md).

**Upgrading a project:**

- **A key a newer twee-tools added** stops an older one, which says so and suggests upgrading. Add
  `requires = "0.2"` to say outright which twee-tools a project needs.
- **A key that's renamed or retired** keeps working, with a warning saying what replaces it, for at least one minor
  version.
- **Story formats** change built pages most of all. `format = "SugarCube@2.37.3"` asks for at least that version, as
  Tweego reads it, so a newer twee-tools with a newer SugarCube builds with that, and says so. To keep a version for
  good, `twee-tools formats vendor` copies the format into the project, where it wins over the built-in one.

## Development

```sh
cargo test                                 # every crate's tests
cargo clippy --all-targets -- -D warnings  # no warnings allowed
cargo fmt                                  # rustfmt.toml: 160 columns
```

| Crate | What |
|---|---|
| `crates/story` | Reads Twee 3 (and Twee2) and published Twine 2 HTML into one model, `Story`. `markup.rs` scans SugarCube markup; `media.rs` is the media inventory the checker and the compressor share. |
| `crates/check` | The checker. `index.rs` parses each passage once; each check in `checks/` reads that index and reports into an `Out`, and the independent checks run in parallel. `format.rs` decides the story's format, and `harlowe/`, `chapbook.rs` and `snowman.rs` read each format's markup. |
| `crates/build` | The compiler: story formats (built in from `formats/`), the page, archives, JSON, decompiling, fonts, media passages and modules. |
| `crates/media` | Re-encodes images as WebP, and rewrites a story's paths in a copy. |
| `crates/cli` | The `twee-tools` program: its commands, `twee.toml` and the `dev` server. |

- **Terms** used across the code (story, passage, index, link graph, media inventory, format, …) are defined in
  `CONTEXT.md`.
- **Fixtures:** `fixtures/small` is one story as Twee and as HTML, and `fixtures/seeded` has one of each problem.
  `fixtures/external` holds clones of Extwee's and twee-ts's repos, for their test stories (`git clone --depth 1` each;
  the tests skip without them).
- **Updating a built-in format:** replace its folder in `crates/build/formats/`. For Harlowe, then run
  `python3 scripts/harlowe-macros.py` to regenerate its macro table; for Chapbook, update the patterns in
  `crates/check/src/chapbook.rs`. A test fails until the two agree.
- **Releasing:** add each change to `CHANGELOG.md`'s Unreleased section as it's made. `scripts/release.sh 0.2.0`
  sets the version, closes that section, runs the tests, commits and tags; pushing the tag
  (`git push origin main v0.2.0`) runs `.github/workflows/release.yml`, which builds the five binaries, tries the
  install scripts and publishes the GitHub release. Running that workflow by hand builds everything and publishes
  nothing. `scripts/notices.py` writes the third-party notices each archive carries.
- **Scripts:** `scripts/boots.mjs` loads a built story in headless Chrome and says whether it started;
  `scripts/same-images.mjs` plays every passage and records its images, to compare a story with its compressed copy.

## Licences

The built-in story formats are their authors' work, under their own licences: SugarCube (BSD-2-Clause, Thomas Michael
Edwards), Harlowe (zlib, Leon Arnott), Chapbook (MIT, Chris Klimas) and Snowman (MIT, Chris Klimas and Dan Cox). Their
licence texts are in `crates/build/formats/`, and `twee-tools formats licenses` prints them.

twee-tools itself is licensed under either of

- the [Apache License, Version 2.0](LICENSE-APACHE), or
- the [MIT License](LICENSE-MIT),

at your option. Unless you say otherwise, any contribution you submit for inclusion in twee-tools is licensed as
above, without any additional terms or conditions.

**Your stories are yours.** twee-tools' licence doesn't reach the stories built with it: a built page holds your story
and the story format, under the format's own licence, and no twee-tools code. Publish and sell them however you like.
