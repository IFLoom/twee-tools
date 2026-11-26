//! Compares two published stories passage by passage: `cargo run -p build --example same -- a.html b.html`.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = story::Story::read(std::path::Path::new(&args[1])).unwrap();
    let b = story::Story::read(std::path::Path::new(&args[2])).unwrap();
    let key = |s: &story::Story| {
        let mut v: Vec<(String, Vec<String>, String)> = s.passages.iter().map(|p| (p.name.clone(), p.tags.clone(), p.text.trim().to_string())).collect();
        v.sort();
        v
    };
    let (ka, kb) = (key(&a), key(&b));
    println!("passages: {} vs {}", ka.len(), kb.len());
    let names_a: std::collections::BTreeSet<_> = ka.iter().map(|p| p.0.clone()).collect();
    let names_b: std::collections::BTreeSet<_> = kb.iter().map(|p| p.0.clone()).collect();
    println!("only in the first: {:?}", names_a.difference(&names_b).collect::<Vec<_>>());
    println!("only in the second: {:?}", names_b.difference(&names_a).collect::<Vec<_>>());
    let differ: Vec<&String> = ka.iter().filter(|p| names_b.contains(&p.0) && !kb.contains(p)).map(|p| &p.0).collect();
    println!("same name, different tags or text: {differ:?}");
    let js = |s: &story::Story| s.scripts.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join("");
    let css = |s: &story::Story| s.stylesheets.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join("");
    println!(
        "script same: {} ({} vs {} bytes); stylesheet same: {} ({} vs {} bytes)",
        js(&a) == js(&b),
        js(&a).len(),
        js(&b).len(),
        css(&a) == css(&b),
        css(&a).len(),
        css(&b).len()
    );
    println!("start: {:?} vs {:?}; ifid same: {}; title: {:?} vs {:?}", a.start, b.start, a.ifid == b.ifid, a.title, b.title);
}
