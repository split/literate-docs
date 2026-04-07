use markdown::mdast::Node;
use markdown::{to_mdast, ParseOptions};
use mdast_util_to_markdown::to_markdown;

pub fn parse_markdown(input: &str) -> Node {
    to_mdast(input, &ParseOptions::gfm()).expect("Failed to parse markdown")
}

pub fn render_markdown<F>(input: &str, transform: F) -> String
where
    F: FnOnce(Node) -> Node,
{
    let has_trailing_newline = input.ends_with('\n');

    let ast = parse_markdown(input);
    let transformed = transform(ast);

    let mut output = to_markdown(&transformed).expect("Failed to compile markdown");

    if !has_trailing_newline {
        output = output.trim_end_matches('\n').to_string();
    }

    output
}

pub fn render_markdown_from_ast(ast: &Node) -> String {
    to_markdown(ast)
        .expect("Failed to compile markdown")
        .trim_end_matches('\n')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(ast: Node) -> Node {
        ast
    }

    #[test]
    fn test_render_unmodified() {
        let input = "# Hello World\n\nSome text here.";
        let output = render_markdown(input, identity);
        assert_eq!(output, input);
    }

    #[test]
    fn test_render_preserves_trailing_newline() {
        let input = "Hello\n";
        let output = render_markdown(input, identity);
        assert_eq!(output, input);
    }

    #[test]
    fn test_render_strips_trailing_newline() {
        let input = "Hello";
        let output = render_markdown(input, identity);
        assert_eq!(output, input);
    }
}
