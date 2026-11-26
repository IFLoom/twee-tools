//! How many words a player reads: the prose of the story's passages, without code, markup, or the passages a format runs
//! for itself. Unlike Tweego's count (every character, code included, over five), this counts words.

use crate::format::Format;
use crate::index::Index;
use crate::{chapbook, harlowe, snowman};

/// The words in every passage a player reads.
pub(crate) fn count(ix: &Index) -> usize {
    ix.passages().filter(|(_, p, _)| !ix.format.is_special(p)).map(|(id, p, _)| words(&shown(ix, id, &p.text))).sum()
}

/// A passage's text as a player sees it, roughly: code, macro calls and markup gone, links as their words.
fn shown(ix: &Index, id: crate::index::PassageId, text: &str) -> String {
    let parse = ix.parse(id);
    // The format's own code and markup, blanked to spaces.
    let mut base: Vec<u8> = match ix.format {
        Format::Chapbook => chapbook::prose(text).into_bytes(),
        _ => parse.markup.clone().into_bytes(),
    };
    let mut blank = |from: usize, to: usize| {
        for b in &mut base[from.min(to)..to.min(text.len())] {
            if *b != b'\n' {
                *b = b' ';
            }
        }
    };
    match ix.format {
        Format::Harlowe => parse.calls.iter().for_each(|c| blank(c.start, c.end)),
        Format::Snowman => {
            let mut at = 0;
            while let Some(open) = text[at..].find("<%").map(|i| at + i) {
                let close = text[open..].find("%>").map(|i| open + i + 2).unwrap_or(text.len());
                blank(open, close);
                at = close;
            }
        }
        Format::Chapbook => {}
        _ => parse.tags.iter().for_each(|t| blank(t.start, t.end)),
    }
    let shown = String::from_utf8_lossy(&base).into_owned();
    // Links show their words, not their markup.
    let links = match ix.format {
        Format::Harlowe => harlowe::links(&shown),
        Format::Chapbook => chapbook::links(&shown),
        Format::Snowman => snowman::links(&shown),
        _ => crate::scan::links(&shown).0,
    };
    let mut out = String::with_capacity(shown.len());
    let mut at = 0;
    for link in links {
        if link.start < at {
            continue;
        }
        out.push_str(&shown[at..link.start]);
        out.push(' ');
        out.push_str(&link.text);
        out.push(' ');
        at = link.end;
    }
    out.push_str(&shown[at..]);
    out
}

/// Words in some text: runs of letters and digits, outside HTML tags, and not variables (`$gold`, shown as their value).
fn words(text: &str) -> usize {
    let mut in_tag = false;
    let mut plain = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                plain.push(' ');
            }
            _ if !in_tag => plain.push(c),
            _ => {}
        }
    }
    plain.split_whitespace().filter(|w| !w.starts_with(['$', '_']) && w.chars().any(char::is_alphanumeric)).count()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    fn count(format: &str, passages: &str) -> usize {
        let head = format!(
            ":: StoryTitle\nT\n\n:: StoryData\n{{\"ifid\": \"0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11\", \"format\": \"{format}\", \"start\": \"Start\"}}\n\n"
        );
        let story = story::Story::from_twee(&format!("{head}{passages}"), Path::new("story.twee")).unwrap();
        crate::check(&story, &crate::config::Config::default()).stats.words
    }

    #[test]
    fn words_a_player_reads_in_each_format() {
        // Four words of prose, and a link's two; the code, the macro and the special passage don't count.
        let sugarcube = ":: Start\nThe <b>lamp</b> is lit. <<set $gold to 5>>You have $gold. [[Go on|Hall]]\n\n:: StoryInit\n<<set $x to 1>> Setup words here.\n\n:: Hall\nA hall.";
        assert_eq!(count("SugarCube", sugarcube), 4 + 2 + 2 + 2);
        let harlowe = ":: Start\n(set: $gold to 5)The lamp is lit.(if: $gold > 1)[Rich.] [[Go on->Hall]]\n\n:: Hall\nA hall.";
        assert_eq!(count("Harlowe", harlowe), 4 + 1 + 2 + 2);
        let chapbook = ":: Start\ngold: 5\n--\nThe lamp is lit. {gold} coins.\n[note]\nNot for players.\n[continue]\n[[Go on->Hall]]\n\n:: Hall\nA hall.";
        assert_eq!(count("Chapbook", chapbook), 4 + 1 + 2 + 2);
        let snowman = ":: Start\n<% s.gold = 5 %>The lamp is lit. <%= s.gold %> coins. [[Go on->Hall]]\n\n:: Hall\nA hall.";
        assert_eq!(count("Snowman", snowman), 4 + 1 + 2 + 2);
    }
}
