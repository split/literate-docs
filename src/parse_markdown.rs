use markdown::{to_mdast, ParseOptions};
use markdown::mdast::Node;

pub fn parse_markdown(input: &str) -> Node {
    to_mdast(input, &ParseOptions::default())
        .expect("Failed to parse markdown")
}
