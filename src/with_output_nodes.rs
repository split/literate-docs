use crate::execute_code_blocks::is_executable_node;
use markdown::mdast::{Code, Node};

pub fn is_output_node(node: &Node) -> bool {
    match node {
        Node::Code(c) => c.lang.as_deref() == Some("output"),
        Node::Html(h) => h.value.contains("<!-- output:"),
        _ => false,
    }
}

fn find_output_before_next_executable(children: &[Node], start: usize) -> Option<usize> {
    children[start..]
        .iter()
        .take_while(|c| !is_executable_node(c))
        .position(is_output_node)
        .map(|pos| start + pos)
}

fn create_empty_output_placeholder() -> Node {
    Node::Code(Code {
        value: String::new(),
        lang: Some("output".to_string()),
        meta: None,
        position: None,
    })
}

pub fn with_output_nodes(node: &Node) -> Node {
    fn process_node(node: &Node) -> Node {
        if let Some(children) = node.children() {
            let result = process_children(children);
            let mut owned = node.to_owned();
            if let Some(children_mut) = owned.children_mut() {
                *children_mut = result;
            }
            owned
        } else {
            node.to_owned()
        }
    }

    fn process_children(children: &[Node]) -> Vec<Node> {
        let mut result = Vec::new();
        let mut i = 0;

        while i < children.len() {
            let child = &children[i];

            if is_output_node(child) {
                i += 1;
                continue;
            }

            if is_executable_node(child) {
                let placed = process_node(child);
                result.push(placed);

                if let Some(output_idx) = find_output_before_next_executable(children, i + 1) {
                    for j in (i + 1)..output_idx {
                        result.push(process_node(&children[j]));
                    }
                    result.push(children[output_idx].to_owned());
                    i = output_idx + 1;
                } else {
                    result.push(create_empty_output_placeholder());
                    i += 1;
                }
                continue;
            }

            let placed = process_node(child);
            result.push(placed);
            i += 1;
        }

        result
    }

    process_node(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown::{to_mdast, ParseOptions};

    fn parse(input: &str) -> Node {
        to_mdast(input, &ParseOptions::default()).unwrap()
    }

    #[test]
    fn test_adds_placeholder_after_executable() {
        let input = "```sh exec\necho hello\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        assert_eq!(result.children().unwrap().len(), 2);
        assert!(is_output_node(&result.children().unwrap()[1]));
    }

    #[test]
    fn test_removes_orphan_output_node() {
        let input = "```output\nstale\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        assert_eq!(result.children().unwrap().len(), 0);
    }

    #[test]
    fn test_keeps_valid_output_between_executables() {
        let input = "```sh exec\necho one\n```\n\n```output\none\n```\n\n```sh exec\necho two\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        let children = result.children().unwrap();
        assert_eq!(children.len(), 4);
        assert!(is_output_node(&children[1]));
    }

    #[test]
    fn test_output_separated_by_text() {
        let input = "```sh exec\necho hello\n```\n\nSome text here.\n\n```output\nhello\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        let children = result.children().unwrap();
        assert_eq!(children.len(), 3);
        assert!(is_output_node(&children[2]));
    }

    #[test]
    fn test_output_separated_by_heading() {
        let input = "```sh exec\necho hello\n```\n\n## Results\n\n```output\nhello\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        let children = result.children().unwrap();
        assert_eq!(children.len(), 3);
        assert!(is_output_node(&children[2]));
    }

    #[test]
    fn test_output_separated_by_multiple_nodes() {
        let input = "```sh exec\necho hello\n```\n\nText\n\n## Heading\n\nMore text\n\n```output\nhello\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        let children = result.children().unwrap();
        assert_eq!(children.len(), 5);
        assert!(is_output_node(&children[4]));
    }

    #[test]
    fn test_orphan_output_after_text_removed() {
        let input = "Some text\n\n```output\norphan\n```";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        let children = result.children().unwrap();
        assert_eq!(children.len(), 1);
        assert!(!is_output_node(&children[0]));
    }

    #[test]
    fn test_placeholder_added_when_no_output_in_sight() {
        let input = "```sh exec\necho hello\n```\n\nSome text after.";
        let ast = parse(input);
        let result = with_output_nodes(&ast);
        let children = result.children().unwrap();
        assert_eq!(children.len(), 3);
        assert!(is_output_node(&children[1]));
    }
}
