# twee-tools: domain terms

**Published page**: a story as Twine publishes it, one `.html` file holding every passage and the story format. What
`check` reads, whoever made it: the Twine editor, another tool, or `twee-tools build`.

**Project**: a folder with a `twee.toml`: Twee sources and the rules for building them (what to leave out, which fonts
and media go in, where the page goes). What `build` and `dev` read; `build --check-only` checks it without writing.

**Story**: one Twine story, read from Twee files or a published page (`story::Story`): its passages, its JavaScript and
stylesheets, and its data (title, IFID, format, start). Every command works on one. A name means one passage: when a story declares one twice, it keeps the last
declaration in the first one's place (as Tweego builds it, and SugarCube and Harlowe play a page), and records the others
in `duplicates`, so the checker, the compiler and the decompiler all see the passage that ships.

**Passage**: a named piece of the story with tags and text. A *code passage* is one SugarCube runs rather than shows
(tagged `widget`, `script`, `init`, …, or a special passage such as StoryInit).

**Check**: one stage of the checker, in `check/src/checks/`: a plain function that reads the index and reports into an
out (`links`, `variables`, `dead_ends`, …). `check_with` runs them in order. A check never changes the story.

**Index**: the story as every check reads it (`check/src/index.rs`): each passage parsed once into a *passage parse*,
and the passages' names indexed. Read-only, and built once per check run. A `PassageId` is a passage's place in it;
only the index makes one.

**Passage parse**: a passage's markup (code and comments blanked), its code (comments blanked) and its macro tags.

**Out**: what checks report into (`check/src/out.rs`): findings, the counts (`Stats`) and the media summary. A finding
is made only by its constructors: `at` (a passage and an offset), `story_data` (the story as a whole) and `in_file` (a
line of any file).

**Finding**: one thing the checker reports: severity (error, warning, note), a code such as `broken-link`, the
message, where it is, and optionally its subject, a hint and a size.

**Link graph**: where the story's passages lead (`check/src/graph.rs`), built once from the index: every *link site* (a
passage, an offset, a target as written and how it links: a link, an image link, a macro, an include, `data-passage` or
code), the edges between passages, the dynamic sites (targets only known as the story runs), the includes, and the
*roots* the story starts from. *Reached* is a query on it. The broken-link, unreachable and dead-end checks report
from it.

**Story macros**: the macros a story defines itself (`check/src/macros_defined.rs`), found once with the index: each
widget and each name `Macro.add` gives, in the story's JavaScript or a passage, with its kind (widget or script), where
it is, whether it's a container, and its definition. Every check that asks whether the story defines `<<x>>` reads it.

**Media inventory**: a story's media (`story/src/media.rs`): every path it names (a *media reference*, certain where only a
path can stand, dynamic where it's built as the story runs), the media files in its *media root* (the folder the page is
in, or will be built in), and *usage*: which files are used, which nothing names, and the folders it builds paths in.
The checker turns it into findings, the compressor into a plan; neither works out "used" itself.

**Format**: the story's format as the checker sees it (`check/src/format.rs`), decided once per check: SugarCube (also
a story that names none, which `build` builds as SugarCube), Harlowe, Chapbook, Snowman or other. It says whose rules
apply, which passages and tags the format runs without a link (Twine's own `StoryTitle`, `StoryData` and media
passages in every format), and which macro arguments name a passage. No check tests the format's name itself.
