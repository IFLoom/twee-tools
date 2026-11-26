//! Story variables down to their properties: `$office.reputation`, not only `$office`. Which paths the story sets (by
//! `<<set>>`, `=`, a receiver, or the keys of an object literal), which it reads, and which it sets in a way that can't be
//! followed (from code, a computed key, a spread), so their properties go unchecked.

use std::collections::{BTreeMap, HashMap};

/// What a set gives a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// An object whose properties are known from literals and sets.
    Object,
    /// A number, string, true/false or the like.
    Leaf,
    /// Something whose properties can't be known: a function's result, a copy, data from JavaScript.
    Opaque,
}

/// One `$path` in some text.
#[derive(Debug, Clone, PartialEq)]
pub struct Use {
    pub offset: usize,
    /// `["office", "reputation"]`.
    pub path: Vec<String>,
    pub set: bool,
    pub read: bool,
    /// The chain went on through something computed: `$a[x]`.
    pub computed: bool,
    /// For a set, what it gives the path, and the paths an object literal adds below it.
    pub shape: Option<Shape>,
    pub keys: Vec<Vec<String>>,
}

/// Properties every value has, or that a story reads off arrays and strings: not the story's own.
const BUILT_IN: &[&str] = &["length", "size", "constructor", "prototype", "__proto__"];

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Every `$name.with.properties` in text (comments already blanked).
pub fn uses(code: &str) -> Vec<Use> {
    let unsets: Vec<usize> = code.match_indices("<<unset").map(|(at, _)| at).collect();
    let bytes = code.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let starts = bytes[i] == b'$' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') && (i == 0 || !is_word(bytes[i - 1]));
        if !starts {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 1;
        while j < bytes.len() && is_word(bytes[j]) && bytes[j] != b'$' {
            j += 1;
        }
        let mut path = vec![code[i + 1..j].to_string()];
        let mut computed = false;
        // .name and ["name"] follow the path; .name( is a method, and [x] is computed.
        loop {
            if bytes.get(j) == Some(&b'.') && bytes.get(j + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_') {
                let mut k = j + 1;
                while k < bytes.len() && is_word(bytes[k]) && bytes[k] != b'$' {
                    k += 1;
                }
                if bytes.get(k) == Some(&b'(') || BUILT_IN.contains(&&code[j + 1..k]) {
                    break;
                }
                path.push(code[j + 1..k].to_string());
                j = k;
            } else if bytes.get(j) == Some(&b'[') {
                let rest = &code[j + 1..];
                let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'));
                let literal = quote.and_then(|q| rest[1..].find(q).map(|end| (end, q))).filter(|(end, _)| rest[1 + end + 1..].starts_with(']'));
                match literal {
                    Some((end, _)) if !rest[1..1 + end].contains(['$', '+']) => {
                        path.push(rest[1..1 + end].to_string());
                        j += 1 + 1 + end + 1 + 1;
                    }
                    _ => {
                        computed = true;
                        break;
                    }
                }
            } else {
                break;
            }
        }
        let after = code[j..].trim_start_matches([' ', '\t']);
        let before = code[..start].trim_end_matches([' ', '\t']);
        let assigns = (after.starts_with("to ") || after.starts_with("to\t") || after.starts_with("to\n"))
            || (after.starts_with('=') && !after.starts_with("==") && !after.starts_with("=>"));
        let updates = ["+=", "-=", "*=", "/=", "%=", "++", "--", "??=", "||=", "&&=", "**="].iter().any(|op| after.starts_with(op))
            || before.ends_with("++")
            || before.ends_with("--");
        let quoted_before = start > 0 && matches!(bytes[start - 1], b'"' | b'\'' | b'`');
        let receiver = quoted_before && {
            let head = before.trim_end_matches(['"', '\'', '`']).trim_end();
            ["textbox", "textarea", "numberbox", "checkbox", "radiobutton", "listbox", "cycle"].iter().any(|m| head.ends_with(&format!("<<{m}")))
        };
        let unset = crate::inside_unset(code, &unsets, start) || before.ends_with("delete");
        let set = assigns || updates || receiver || unset;
        let read = !(assigns || receiver || unset);
        let (shape, keys) = if assigns {
            let value = after.strip_prefix("to").unwrap_or(after).trim_start_matches('=').trim_start();
            value_shape(value)
        } else if set {
            (Some(Shape::Leaf), vec![])
        } else {
            (None, vec![])
        };
        out.push(Use { offset: start, path, set, read, computed, shape, keys });
        i = j.max(i + 1);
    }
    out
}

/// What a value gives the path it's set to, and the key paths an object literal holds.
fn value_shape(value: &str) -> (Option<Shape>, Vec<Vec<String>>) {
    let first = value.chars().next().unwrap_or(' ');
    if first == '{' {
        return match object_keys(value) {
            Some(keys) => (Some(Shape::Object), keys),
            None => (Some(Shape::Opaque), vec![]),
        };
    }
    let leaf = first.is_ascii_digit()
        || matches!(first, '"' | '\'' | '`' | '-')
        || ["true", "false", "null", "undefined", "NaN"].iter().any(|w| value.starts_with(w) && !value[w.len()..].starts_with(|c: char| c.is_alphanumeric()));
    // `$a to $b + 1` and the like is a leaf too; a call, a copy, an array or another variable is opaque.
    (Some(if leaf { Shape::Leaf } else { Shape::Opaque }), vec![])
}

/// The key paths of an object literal at the start of text, `{ a: 1, b: { c: 2 } }` → a, b, b.c; None for a spread or a
/// computed key, whose keys can't be known.
pub fn object_keys(text: &str) -> Option<Vec<Vec<String>>> {
    let body = matching(text)?;
    let mut out = vec![];
    for entry in split_top(&body[1..body.len() - 1], b',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        if entry.starts_with("...") || entry.starts_with('[') {
            return None;
        }
        let colon = top_colon(entry);
        let (key, value) = match colon {
            Some(at) => (entry[..at].trim(), entry[at + 1..].trim()),
            // Shorthand `{ name }` or a method `name() { }`.
            None => (entry.split(['(', ' ']).next().unwrap_or(entry).trim(), ""),
        };
        let key = key.trim_matches(['"', '\'', '`']).to_string();
        if key.is_empty() {
            continue;
        }
        out.push(vec![key.clone()]);
        if value.starts_with('{') {
            for sub in object_keys(value)? {
                let mut path = vec![key.clone()];
                path.extend(sub);
                out.push(path);
            }
        }
    }
    Some(out)
}

/// The text from an opening bracket to its match, past strings and nested brackets.
fn matching(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(_) if b == b'\\' => i += 1,
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' | b'`' => quote = Some(b),
                b'{' | b'[' | b'(' => depth += 1,
                b'}' | b']' | b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[..=i]);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

/// Splits text at a separator outside strings and brackets.
fn split_top(text: &str, separator: u8) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut parts = vec![];
    let (mut depth, mut quote, mut last) = (0i32, None::<u8>, 0);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(_) if b == b'\\' => i += 1,
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None => match b {
                b'"' | b'\'' | b'`' => quote = Some(b),
                b'{' | b'[' | b'(' => depth += 1,
                b'}' | b']' | b')' => depth -= 1,
                _ if b == separator && depth == 0 => {
                    parts.push(&text[last..i]);
                    last = i + 1;
                }
                _ => {}
            },
        }
        i += 1;
    }
    parts.push(&text[last..]);
    parts
}

fn top_colon(entry: &str) -> Option<usize> {
    let parts = split_top(entry, b':');
    (parts.len() > 1).then(|| parts[0].len())
}

/// What the story gives each path, from every set.
#[derive(Default, Debug)]
pub struct Known {
    shapes: HashMap<String, Shape>,
}

impl Known {
    fn mark(&mut self, path: &[String], shape: Shape) {
        let key = path.join(".");
        let entry = self.shapes.entry(key).or_insert(shape);
        // Opaque wins: once something can't be followed, nothing below it is checked.
        if shape == Shape::Opaque || (*entry == Shape::Leaf && shape == Shape::Object) {
            *entry = shape;
        }
    }

    pub fn set(&mut self, use_: &Use) {
        let path = &use_.path;
        for depth in 1..path.len() {
            self.mark(&path[..depth], Shape::Object);
        }
        if use_.computed {
            self.mark(path, Shape::Opaque);
            return;
        }
        self.mark(path, use_.shape.unwrap_or(Shape::Leaf));
        for key in &use_.keys {
            let mut full = path.clone();
            full.extend(key.iter().cloned());
            for depth in path.len() + 1..full.len() {
                self.mark(&full[..depth], Shape::Object);
            }
            self.mark(&full, Shape::Leaf);
        }
    }

    pub fn opaque(&mut self, root: &str) {
        self.mark(&[root.to_string()], Shape::Opaque);
    }

    /// For a property read the story never sets: None when it's set, or under something that can't be followed.
    pub fn missing(&self, path: &[String]) -> Option<String> {
        if path.len() < 2 || !self.shapes.contains_key(&path[0]) {
            return None;
        }
        for depth in 1..=path.len() {
            match self.shapes.get(&path[..depth].join(".")) {
                Some(Shape::Opaque) => return None,
                Some(_) => {}
                // A read below a leaf (a string's own property) isn't the story's.
                None if depth > 1 && self.shapes.get(&path[..depth - 1].join(".")) == Some(&Shape::Leaf) => {
                    return None;
                }
                None => return Some(path[..depth].join(".")),
            }
        }
        None
    }

    /// Paths under the same variable that end in the same property: `$player.char` → `player.stats.char`.
    pub fn same_name_under(&self, path: &str) -> Vec<String> {
        let root = path.split('.').next().unwrap_or(path);
        let last = path.rsplit('.').next().unwrap_or(path);
        let mut out: Vec<String> = self
            .shapes
            .keys()
            .filter(|k| k.as_str() != path && k.split('.').next() == Some(root) && k.rsplit('.').next() == Some(last) && k.contains('.'))
            .cloned()
            .collect();
        out.sort_by_key(|k| (k.len(), k.clone()));
        out
    }

    /// The properties set beside a missing one, for "did you mean".
    pub fn siblings(&self, path: &str) -> Vec<String> {
        let parent = path.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
        let mut out: Vec<String> = self.shapes.keys().filter(|k| k.rsplit_once('.').map(|(p, _)| p) == Some(parent) && k.as_str() != path).cloned().collect();
        out.sort();
        out
    }
}

/// How many single-character edits turn one name into the other (3 or more is "far").
pub fn edits(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 2 {
        return 3;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != cb));
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// The closest of some names to one, if it's a typo away: at most 2 edits, and fewer than half the name.
pub fn closest<'a>(name: &str, among: impl IntoIterator<Item = &'a String>) -> Option<&'a String> {
    let mut best: BTreeMap<usize, &String> = BTreeMap::new();
    for other in among {
        let d = edits(&name.to_lowercase(), &other.to_lowercase());
        if other != name && d <= 2 && d * 2 < name.chars().count() {
            best.entry(d).or_insert(other);
        }
    }
    best.into_values().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> Vec<String> {
        p.split('.').map(str::to_string).collect()
    }

    #[test]
    fn follows_paths_and_literals() {
        let found = uses(
            r#"<<set $office to { reputation: 0, "staff": { count: 2 }, list: [1, 2] }>> <<print $office.staff.count>> $office.items.push(1) $a[$k].b <<set $p.name to "x">> $s.length"#,
        );
        assert_eq!(found[0].path, path("office"));
        assert_eq!(found[0].shape, Some(Shape::Object));
        assert_eq!(found[0].keys, vec![path("reputation"), path("staff"), path("staff.count"), path("list")]);
        assert_eq!((found[1].path.clone(), found[1].read), (path("office.staff.count"), true));
        assert_eq!(found[2].path, path("office.items"), "a method call ends the path");
        assert!(found[3].computed);
        assert_eq!(found[4].path, path("k"), "the variable inside $a[$k] is a read of its own");
        assert_eq!((found[5].path.clone(), found[5].set, found[5].shape), (path("p.name"), true, Some(Shape::Leaf)));
        assert_eq!(found[6].path, path("s"), "built-in properties aren't the story's");
    }

    #[test]
    fn knows_what_is_missing() {
        let mut known = Known::default();
        for u in
            uses(r#"<<set $office to { reputation: 0, staff: {} }>> <<set $office.staff.count to 1>> <<set $copy to clone($office)>> <<set $name to "Ava">>"#)
        {
            if u.set {
                known.set(&u);
            }
        }
        assert_eq!(known.missing(&path("office.reputation")), None);
        assert_eq!(known.missing(&path("office.staff.count")), None);
        assert_eq!(known.missing(&path("office.reputaton")).as_deref(), Some("office.reputaton"));
        assert_eq!(known.missing(&path("office.wing.size")).as_deref(), Some("office.wing"));
        assert_eq!(known.missing(&path("copy.anything")), None, "a copy can't be followed");
        assert_eq!(known.missing(&path("name.first")), None, "a string's own property");
        assert_eq!(known.missing(&path("never.set")), None, "a root nothing sets is reported elsewhere");
        assert_eq!(known.same_name_under("office.count"), vec!["office.staff.count".to_string()]);
        let siblings = known.siblings("office.reputaton");
        assert_eq!(
            closest("reputaton", siblings.iter().map(|s| s.rsplit('.').next().unwrap().to_string()).collect::<Vec<_>>().iter()).map(String::as_str),
            Some("reputation")
        );
    }

    #[test]
    fn spreads_and_computed_keys_are_opaque() {
        assert_eq!(object_keys("{ a: 1, ...b }"), None);
        assert_eq!(object_keys("{ [k]: 1 }"), None);
        assert_eq!(object_keys("{ a: 'x, y', b: f(1, 2) }"), Some(vec![path("a"), path("b")]));
    }
}
