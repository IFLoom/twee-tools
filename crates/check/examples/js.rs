//! For a story and a passage, the arguments of each <<set>>, <<run>> and <<if>> that don't parse, as the checker sees them:
//! `cargo run --example js -- <story> <passage>`.
use check::scan::{self, Keep};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let story = story::Story::read(std::path::Path::new(&args[1])).unwrap();
    let passage = story.passage(&args[2]).expect("no such passage");
    let markup = scan::blank(&passage.text, Keep::Markup);
    for tag in scan::macros(&markup).iter().filter(|t| !t.closing && matches!(t.name.as_str(), "set" | "run" | "if" | "elseif")) {
        let error = if matches!(tag.name.as_str(), "set" | "run") { check::js::statements(&tag.args) } else { check::js::expression(&tag.args) };
        if let Some(error) = error {
            println!("<<{}>> line {}: {:?}\n  args: {:?}", tag.name, passage.line_at(tag.start), error, tag.args);
        }
    }
}
