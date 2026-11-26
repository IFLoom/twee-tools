//! SugarCube 2.37's own macros: which close with a tag, and which may only sit inside another.

/// Macros that wrap content and need a closing tag.
pub const CONTAINERS: &[&str] = &[
    "capture",
    "do",
    "nobr",
    "silent",
    "silently",
    "type",
    "script",
    "for",
    "if",
    "switch",
    "button",
    "cycle",
    "link",
    "linkappend",
    "linkprepend",
    "linkreplace",
    "listbox",
    "createaudiogroup",
    "createplaylist",
    "done",
    "repeat",
    "timed",
    "widget",
    "append",
    "prepend",
    "replace",
    "click",
];

/// Macros that close nothing.
pub const SINGLES: &[&str] = &[
    "=",
    "-",
    "include",
    "display",
    "print",
    "run",
    "set",
    "unset",
    "goto",
    "back",
    "return",
    "choice",
    "actions",
    "addclass",
    "removeclass",
    "toggleclass",
    "remove",
    "copy",
    "audio",
    "cacheaudio",
    "masteraudio",
    "playlist",
    "removeaudiogroup",
    "removeplaylist",
    "waitforaudio",
    "setplaylist",
    "stopallaudio",
    "checkbox",
    "numberbox",
    "radiobutton",
    "textarea",
    "textbox",
    "redo",
    "break",
    "continue",
    "stop",
    "else",
    "elseif",
    "case",
    "default",
    "option",
    "optionsfrom",
    "track",
    "next",
    "forget",
    "remember",
];

/// A child tag and the containers it may sit inside.
pub fn parents_of(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "else" | "elseif" => &["if"],
        "case" | "default" => &["switch"],
        "option" | "optionsfrom" => &["cycle", "listbox"],
        "track" => &["createaudiogroup", "createplaylist"],
        "next" => &["timed"],
        "stop" => &["repeat"],
        "break" | "continue" => &["for"],
        _ => return None,
    })
}

/// Macro arguments that name a passage: (macro, which argument).
pub fn passage_args(name: &str, count: usize) -> &'static [usize] {
    match name {
        "goto" | "include" | "display" | "choice" => &[0],
        "link" | "button" | "click" if count >= 2 => &[1],
        "back" | "return" if count >= 2 => &[1],
        "textbox" | "numberbox" if count >= 3 => &[2],
        "actions" => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        _ => &[],
    }
}

/// Passages SugarCube runs or shows by name, so nothing needs to link to them.
pub const SPECIAL_PASSAGES: &[&str] = &[
    "StoryInit",
    "StoryCaption",
    "StoryBanner",
    "StoryMenu",
    "StorySubtitle",
    "StoryAuthor",
    "StoryDisplayTitle",
    "StoryInterface",
    "StoryShare",
    "StoryTitle",
    "StoryData",
    "PassageReady",
    "PassageDone",
    "PassageHeader",
    "PassageFooter",
    "MenuOptions",
    "MenuShare",
    "MenuStory",
];

/// Tags that make a passage run or load without a link.
pub const SPECIAL_TAGS: &[&str] = &["widget", "init", "script", "stylesheet"];

/// SugarCube's own functions, which a story can call by name: `hasVisited("Room")`, `either(…)`.
pub const FUNCTIONS: &[&str] = &[
    "clone",
    "either",
    "forget",
    "hasVisited",
    "importScripts",
    "importStyles",
    "lastVisited",
    "memorize",
    "passage",
    "previous",
    "random",
    "randomFloat",
    "recall",
    "setPageElement",
    "tags",
    "temporary",
    "time",
    "triggerEvent",
    "turns",
    "variables",
    "visited",
    "visitedTags",
];

/// A SugarCube feature and the version it came in, from each entry's History in the SugarCube 2 docs.
pub type Since = (&'static str, (u32, u32, u32));

pub const SINCE_MACROS: &[(&str, Since)] = &[
    ("silent", ("<<silent>>", (2, 37, 0))),
    ("do", ("<<do>>", (2, 37, 0))),
    ("redo", ("<<redo>>", (2, 37, 0))),
    ("type", ("<<type>>", (2, 32, 0))),
    ("cycle", ("<<cycle>>", (2, 29, 0))),
    ("listbox", ("<<listbox>>", (2, 26, 0))),
    ("done", ("<<done>>", (2, 35, 0))),
];

pub const SINCE_TAGS: &[(&str, Since)] = &[("init", ("The init tag", (2, 36, 0))), ("Twine.audio", ("Twine.audio passages", (2, 24, 0)))];

pub const SINCE_PASSAGES: &[(&str, Since)] = &[("StoryDisplayTitle", ("StoryDisplayTitle", (2, 31, 0)))];

/// Text in the story's JavaScript, the feature it is, and the version it came in.
pub const SINCE_CODE: &[(&str, &str, (u32, u32, u32))] =
    &[("Save.browser", "Save.browser", (2, 37, 0)), ("maxAutoSaves", "Config.saves.maxAutoSaves", (2, 37, 0)), ("Template.add", "Template.add", (2, 29, 0))];

/// Tags that make a passage code: SugarCube won't show it, and won't include it.
pub const CODE_TAGS: &[&str] = &["widget", "init", "script", "stylesheet", "Twine.image", "Twine.audio", "Twine.video", "Twine.vtt"];

/// Special passages that can't be navigated to.
pub const CODE_PASSAGES: &[&str] = &[
    "StoryInit",
    "PassageReady",
    "PassageDone",
    "PassageHeader",
    "PassageFooter",
    "StoryCaption",
    "StoryMenu",
    "StoryBanner",
    "StoryInterface",
    "StoryDisplayTitle",
    "StorySubtitle",
    "StoryAuthor",
    "StoryData",
    "StoryTitle",
    "StoryShare",
];

/// Special passages that generate no output.
pub const NO_OUTPUT: &[&str] = &["StoryInit", "PassageReady", "PassageDone"];

/// Macros SugarCube 2.37 removed, and what to use.
pub const REMOVED_MACROS: &[(&str, &str)] = &[
    ("click", "use <<link>>"),
    ("display", "use <<include>>"),
    ("forget", "use the forget() function"),
    ("remember", "use memorize() and recall()"),
    ("setplaylist", "use <<createplaylist>>"),
    ("stopallaudio", "use <<audio \":all\" stop>>"),
];

/// Macros that still work and are deprecated, and what to use.
pub const DEPRECATED_MACROS: &[(&str, &str)] =
    &[("silently", "use <<silent>>"), ("actions", "use <<link>> or link markup"), ("choice", "use <<link>> or link markup")];

/// APIs SugarCube 2.37 removed, as they're written in code, and what to use.
pub const REMOVED_API: &[(&str, &str)] = &[
    ("tale.", "use the Story API"),
    ("TempVariables", "use temporary variables (_name)"),
    ("History.", "use the State API"),
    ("Array.random(", "use <Array>.random()"),
    (".containsAll(", "use <Array>.includesAll()"),
    (".containsAny(", "use <Array>.includesAny()"),
    (".flatten(", "use <Array>.flat(Infinity)"),
    ("readBracketedList(", "use String.prototype.split or an array"),
    ("State.backward(", "use Engine.backward()"),
    ("State.forward(", "use Engine.forward()"),
    ("State.display(", "use Engine.play()"),
    ("State.play(", "use Engine.play()"),
    ("State.show(", "use Engine.show()"),
    ("State.restart(", "use Engine.restart()"),
    ("State.initPRNG(", "use State.prng.init()"),
    ("Save.get(", "use the Save.browser API"),
    ("UI.addClickHandler(", "use Dialog.create() and an event handler"),
    ("UI.body(", "use Dialog.body()"),
    ("UI.close(", "use Dialog.close()"),
    ("UI.isOpen(", "use Dialog.isOpen()"),
    ("UI.open(", "use Dialog.open()"),
    ("UI.resize(", "use Dialog.resize()"),
    ("UI.setStoryElements(", "use UI.update()"),
    ("UI.setup(", "use Dialog.create()"),
    ("UI.stow(", "use UIBar.stow()"),
    ("UI.unstow(", "use UIBar.unstow()"),
    ("Dialog.addClickHandler(", "use Dialog.create() and an event handler"),
];

/// APIs that still work and are deprecated, as they're written in code, and what to use.
pub const DEPRECATED_API: &[(&str, &str)] = &[
    ("predisplay[", "use the :passagestart event"),
    ("prerender[", "use the :passagerender event"),
    ("postrender[", "use the :passagerender event"),
    ("postdisplay[", "use the :passagedisplay event"),
    ("prehistory[", "use the :passageinit event"),
    ("Config.macros.ifAssignmentError", "use Config.enableOptionalDebugging"),
    ("Config.passages.descriptions", "use Config.saves.metadata"),
    ("Config.saves.autoload", "use the Save.browser API"),
    ("Config.saves.autosave", "use Config.saves.maxAutoSaves"),
    ("Config.saves.onLoad", "use Save.onLoad.add()"),
    ("Config.saves.onSave", "use Save.onSave.add()"),
    ("Config.saves.slots", "use Config.saves.maxSlotSaves"),
    ("Config.saves.tryDiskOnMobile", "it's no longer needed"),
    ("Dialog.setup(", "use Dialog.create()"),
    ("UI.jumpto(", "use the history API"),
    ("UI.share(", "it's going away with StoryShare"),
    ("UI.buildAutoload(", "use the Save.browser API"),
    ("UIBar.update(", "use UI.update()"),
    ("Story.lookup(", "use Story.filter()"),
    ("Story.lookupWith(", "use Story.filter()"),
    ("Story.title", "use Story.name"),
    ("Story.domId", "use Story.id"),
    ("JSON.reviveWrapper(", "use Serial.createReviver()"),
    ("Save.clear(", "use Save.browser.clear()"),
    ("Save.ok(", "use Save.browser.isEnabled()"),
    ("Save.autosave.", "use Save.browser.auto"),
    ("Save.slots.", "use Save.browser.slot"),
    ("Save.export(", "use Save.disk.save()"),
    ("Save.import(", "use Save.disk.load()"),
    ("Save.serialize(", "use Save.base64.save()"),
    ("Save.deserialize(", "use Save.base64.load()"),
    ("Scripting.parse(", "use Scripting.desugar()"),
];

/// The settings SugarCube 2.37 documents, after `Config.`.
pub const CONFIG: &[&str] = &[
    "addVisitedLinkClass",
    "audio.pauseOnFadeToZero",
    "audio.preloadMetadata",
    "cleanupWikifierOutput",
    "debug",
    "enableOptionalDebugging",
    "history.controls",
    "history.maxStates",
    "loadDelay",
    "macros.ifAssignmentError",
    "macros.maxLoopIterations",
    "macros.typeSkipKey",
    "macros.typeVisitedPassages",
    "navigation.override",
    "passages.descriptions",
    "passages.displayTitles",
    "passages.nobr",
    "passages.onProcess",
    "passages.start",
    "passages.transitionOut",
    "saves.autoload",
    "saves.autosave",
    "saves.descriptions",
    "saves.id",
    "saves.isAllowed",
    "saves.maxAutoSaves",
    "saves.maxSlotSaves",
    "saves.metadata",
    "saves.onLoad",
    "saves.onSave",
    "saves.slots",
    "saves.tryDiskOnMobile",
    "saves.version",
    "ui.stowBarInitially",
    "ui.updateStoryElements",
];
