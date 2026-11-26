//! Reading SugarCube markup: blanking what isn't markup, then finding links, macros, strings and media paths in what's left.
//! Every offset is a byte offset into the passage's text, and blanking keeps lengths and newlines, so offsets stay true.

/// What to blank: comments always; code and verbatim text too for `Markup`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Everything but comments: for variables and strings, which code uses too.
    Code,
    /// Only markup: for links, macros and HTML.
    Markup,
}

/// A copy of `text` with each region `keep` drops replaced by spaces (newlines kept).
pub fn blank(text: &str, keep: Keep) -> String {
    let mut out = text.as_bytes().to_vec();
    let lower = text.to_ascii_lowercase();
    let mut regions: Vec<(&str, &str, bool)> = vec![("/*", "*/", true), ("/%", "%/", true), ("<!--", "-->", true)];
    if keep == Keep::Markup {
        regions.extend([
            ("\"\"\"", "\"\"\"", false),
            ("<nowiki>", "</nowiki>", false),
            ("{{{", "}}}", false),
            ("<<script>>", "<</script>>", false),
            ("<script", "</script>", false),
            ("<style", "</style>", false),
        ]);
    }
    // Where each opener next appears, found again only once the scan has passed it: one pass over the text, not one per
    // region found (which made a long script quadratic).
    let mut next: Vec<Option<usize>> = regions.iter().map(|&(open, _, _)| lower.find(open)).collect();
    let mut at = 0;
    while at < text.len() {
        for (slot, &(open, _, _)) in next.iter_mut().zip(&regions) {
            if slot.is_some_and(|p| p < at) {
                *slot = lower[at..].find(open).map(|i| at + i);
            }
        }
        let found = next.iter().zip(&regions).filter_map(|(slot, &(open, close, whole))| slot.map(|p| (p, open, close, whole))).min_by_key(|r| r.0);
        let Some((start, open, close, whole)) = found else {
            break;
        };
        let inner = start + open.len();
        let end = lower[inner..].find(close).map(|i| inner + i).unwrap_or(text.len());
        // Comments go entirely; code keeps its tags (so <<script>> still balances) and loses its body.
        let (from, to) = if whole { (start, (end + close.len()).min(text.len())) } else { (body_start(&lower, start, open), end) };
        for byte in &mut out[from..to] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        at = (end + close.len()).min(text.len()).max(at + 1);
    }
    // Only ASCII bytes were replaced, but a multi-byte character may have been cut; keep it valid.
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Where a region's body starts: after `<script ...>`'s `>`, else after the opener.
fn body_start(lower: &str, start: usize, open: &str) -> usize {
    if open == "<script" || open == "<style" { lower[start..].find('>').map(|i| start + i + 1).unwrap_or(start + open.len()) } else { start + open.len() }
}

/// A `[[...]]` link.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub target: String,
    /// The setter, `[[text|target][$a to 1]]`, and where its code starts.
    pub setter: Option<(usize, String)>,
}

/// An image, `[img[title|src][link][setter]]` (and `[<img[`, `[>img[`).
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub start: usize,
    pub src: String,
    pub src_start: usize,
    pub link: Option<String>,
}

/// Finds `[[...]]` links and `[img[...]]` images in markup.
pub fn links(markup: &str) -> (Vec<Link>, Vec<Image>) {
    let bytes = markup.as_bytes();
    let (mut links, mut images) = (vec![], vec![]);
    let mut at = 0;
    while let Some(i) = markup[at..].find('[') {
        let start = at + i;
        let rest = &markup[start..];
        let lower: String = rest.chars().take(6).collect::<String>().to_ascii_lowercase();
        if rest.starts_with("[[") {
            if let Some((parts, end)) = sections(markup, start + 1) {
                let link = &parts[0].1;
                let (text, target) = split_link(link);
                let setter = parts.get(1).map(|(at, code)| (*at, code.clone()));
                links.push(Link { start, end, text, target, setter });
                at = end;
                continue;
            }
        } else if lower.starts_with("[img[") || lower.starts_with("[<img[") || lower.starts_with("[>img[") {
            let open = start + lower.find('[').map(|_| lower[1..].find('[').unwrap() + 1).unwrap();
            if let Some((parts, end)) = sections(markup, open) {
                let (src_off, first) = &parts[0];
                let (src_rel, src) = match first.rfind('|') {
                    Some(bar) => (bar + 1, first[bar + 1..].to_string()),
                    None => (0, first.clone()),
                };
                images.push(Image { start, src: src.trim().to_string(), src_start: src_off + src_rel, link: parts.get(1).map(|p| p.1.trim().to_string()) });
                at = end;
                continue;
            }
        }
        at = start + 1;
        let _ = bytes;
    }
    (links, images)
}

/// From an opening `[` that starts `[a][b][c]]`, the sections and their offsets, and the end after `]]`.
fn sections(markup: &str, open: usize) -> Option<(Vec<(usize, String)>, usize)> {
    let bytes = markup.as_bytes();
    let mut parts = vec![];
    let mut at = open;
    loop {
        if bytes.get(at) != Some(&b'[') {
            return None;
        }
        let begin = at + 1;
        let mut depth = 0;
        let mut i = begin;
        loop {
            match bytes.get(i)? {
                b'[' => depth += 1,
                b']' if depth > 0 => depth -= 1,
                b']' => break,
                b'\n' if parts.is_empty() && i - begin > 2000 => return None,
                _ => {}
            }
            i += 1;
        }
        parts.push((begin, markup[begin..i].to_string()));
        match bytes.get(i + 1) {
            Some(b']') => return Some((parts, i + 2)),
            Some(b'[') => at = i + 1,
            _ => return None,
        }
    }
}

/// `text|target`, `text->target`, `target<-text`, or `target`.
pub fn split_link(link: &str) -> (String, String) {
    // As SugarCube's own lexer reads it: left to right, the first `|`, `->` or `<-` divides (a `"…"` string hides one),
    // each part trimmed, and a leading `~` only forces the link to be internal.
    let bytes = link.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    at += if bytes[at] == b'\\' { 2 } else { 1 };
                }
            }
            b'|' => return (link[..at].trim().to_string(), internal(&link[at + 1..])),
            b'-' if bytes.get(at + 1) == Some(&b'>') => return (link[..at].trim().to_string(), internal(&link[at + 2..])),
            b'<' if bytes.get(at + 1) == Some(&b'-') => return (link[at + 2..].trim().to_string(), internal(&link[..at])),
            _ => {}
        }
        at += 1;
    }
    (link.trim().to_string(), internal(link))
}

/// A link's passage, trimmed, without the `~` that forces it to be internal.
fn internal(target: &str) -> String {
    let target = target.trim();
    target.strip_prefix('~').unwrap_or(target).to_string()
}

/// A macro tag: `<<name args>>`, `<</name>>`, or the old `<<endname>>` (which `closing` leaves to the checker).
#[derive(Debug, Clone, PartialEq)]
pub struct MacroTag {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub args: String,
    pub args_start: usize,
    pub closing: bool,
}

pub fn macros(markup: &str) -> Vec<MacroTag> {
    let bytes = markup.as_bytes();
    let mut tags = vec![];
    let mut at = 0;
    while let Some(i) = markup[at..].find("<<") {
        let start = at + i;
        let mut j = start + 2;
        let closing = bytes.get(j) == Some(&b'/');
        if closing {
            j += 1;
        }
        let name_start = j;
        if matches!(bytes.get(j), Some(b'=' | b'-')) && !closing {
            j += 1;
        } else if bytes.get(j).is_some_and(|b| b.is_ascii_alphabetic()) {
            while bytes.get(j).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-') {
                j += 1;
            }
        } else {
            at = start + 2;
            continue;
        }
        let name = markup[name_start..j].to_string();
        // The arguments run to the first `>>` outside a string or a [[link]].
        let args_start = j;
        let mut k = j;
        let mut quote: Option<u8> = None;
        let mut depth = 0;
        let end = loop {
            let Some(&b) = bytes.get(k) else { break None };
            match quote {
                Some(_) if b == b'\\' => k += 1,
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None => match b {
                    b'"' | b'\'' | b'`' => quote = Some(b),
                    b'[' => depth += 1,
                    b']' if depth > 0 => depth -= 1,
                    b'>' if depth == 0 && bytes.get(k + 1) == Some(&b'>') => break Some(k),
                    b'<' if bytes.get(k + 1) == Some(&b'<') => break None,
                    _ => {}
                },
            }
            k += 1;
        };
        match end {
            Some(end) => {
                tags.push(MacroTag { start, end: end + 2, name, args: markup[args_start..end].to_string(), args_start, closing });
                at = end + 2;
            }
            None => at = start + 2,
        }
    }
    tags
}

/// One macro argument as SugarCube splits them.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// A quoted string, its value.
    Str(String),
    /// `[[text|target]]` link markup.
    Link(String, String),
    /// `[img[src][link]]` image markup.
    Image(String, Option<String>),
    /// A bareword or an expression.
    Bare(String),
}

pub fn args(text: &str) -> Vec<Arg> {
    let bytes = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() || b == b',' {
            i += 1;
            continue;
        }
        if matches!(b, b'"' | b'\'' | b'`') {
            let mut j = i + 1;
            let mut value = String::new();
            while j < bytes.len() && bytes[j] != b {
                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                    j += 1;
                }
                let ch = text[j..].chars().next().unwrap();
                value.push(ch);
                j += ch.len_utf8();
            }
            out.push(Arg::Str(value));
            i = j + 1;
            continue;
        }
        if b == b'[' {
            let (found_links, found_images) = links(&text[i..]);
            if let Some(link) = found_links.first().filter(|l| l.start == 0) {
                out.push(Arg::Link(link.text.clone(), link.target.clone()));
                i += link.end;
                continue;
            }
            if let Some(image) = found_images.first().filter(|m| m.start == 0) {
                out.push(Arg::Image(image.src.clone(), image.link.clone()));
                let close = text[i..].find("]]").map(|e| i + e + 2).unwrap_or(bytes.len());
                i = close;
                continue;
            }
        }
        let mut j = i;
        while j < bytes.len() && !bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        out.push(Arg::Bare(text[i..j].to_string()));
        i = j;
    }
    out
}

/// Whether a bareword argument is a literal name rather than code: SugarCube reads such a word as a string.
pub fn is_plain_word(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with(['$', '_'])
        && !matches!(word, "true" | "false" | "null" | "undefined" | "NaN")
        && word.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '.' | ':'))
        && !word.chars().next().unwrap().is_ascii_digit()
}

/// The quoted string literals in a piece of text: (offset of the value, value, the offset just after the closing quote).
pub fn strings(text: &str) -> Vec<(usize, String, usize)> {
    let bytes = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b'"' | b'\'' | b'`') {
            // An apostrophe inside a word ("don't") opens nothing.
            if b == b'\'' && i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b && bytes[j] != b'\n' {
                if bytes[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b {
                out.push((i + 1, text[i + 1..j].to_string(), j + 1));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Every quoted string, and the strings inside those (HTML in JavaScript, code in an attribute: `onclick="go('Room')"`), to any depth.
pub fn all_strings(text: &str) -> Vec<(usize, String, usize)> {
    let mut out = strings(text);
    let mut at = 0;
    while at < out.len() {
        let (offset, value, _) = out[at].clone();
        if value.contains(['\'', '"', '`']) {
            out.extend(strings(&value).into_iter().map(|(o, v, a)| (offset + o, v, offset + a)));
        }
        at += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_link_form_and_images() {
        let (links, images) =
            links("a [[Go]] b [[Go in|Bakery]] [[x->Home]] [[Street<-back]] [[Buy|Home][$c -= 3; $b to true]] [img[pics/a b.png][Home]] [img[Title|p.jpg]]");
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["Go", "Bakery", "Home", "Street", "Home"]);
        assert_eq!(images.iter().map(|i| (i.src.as_str(), i.link.as_deref())).collect::<Vec<_>>(), [("pics/a b.png", Some("Home")), ("p.jpg", None)]);
    }

    #[test]
    fn finds_macros_past_strings_with_angle_brackets() {
        let tags = macros(r#"<<if $a gt 1>>x<<print "a >> b">><</if>><<endif>> << not a macro"#);
        let names: Vec<_> = tags.iter().map(|t| (t.name.as_str(), t.closing)).collect();
        assert_eq!(names, [("if", false), ("print", false), ("if", true), ("endif", false)]);
        assert_eq!(tags[1].args.trim(), r#""a >> b""#);
    }

    #[test]
    fn blanks_comments_and_script_bodies_keeping_offsets() {
        let text = "a /* [[X]] */ b <<script>>[[Y]]<</script>> c";
        let markup = blank(text, Keep::Markup);
        assert_eq!(markup.len(), text.len());
        assert!(!markup.contains("[[X]]") && !markup.contains("[[Y]]"));
        assert!(markup.contains("<<script>>") && markup.contains("<</script>>"));
        assert!(blank(text, Keep::Code).contains("[[Y]]"));
    }

    #[test]
    fn splits_arguments() {
        let parsed = args(r#""Go" 'Home' [[Back|Street]] $x Start"#);
        assert_eq!(
            parsed,
            [Arg::Str("Go".into()), Arg::Str("Home".into()), Arg::Link("Back".into(), "Street".into()), Arg::Bare("$x".into()), Arg::Bare("Start".into())]
        );
    }
}

#[cfg(test)]
mod split_tests {
    use super::split_link;

    #[test]
    fn links_split_as_sugarcube_splits_them() {
        let target = |l: &str| split_link(l).1;
        assert_eq!(target("Hall"), "Hall");
        assert_eq!(target("Go | Hall "), "Hall", "trimmed");
        assert_eq!(target("a|b|c"), "b|c", "the first separator divides");
        assert_eq!(target("a->b->c"), "b->c");
        assert_eq!(target("Hall<-Go"), "Hall");
        assert_eq!(target("\"a|b\"|Hall"), "Hall", "a quoted string hides a separator");
        assert_eq!(target("~Hall"), "Hall", "~ only forces the link internal");
    }
}
