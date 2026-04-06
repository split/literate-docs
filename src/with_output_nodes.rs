use crate::execute_code_blocks::is_executable_node;
use crate::output_node::{
    create_output_placeholder, is_output_node, output_format, OutputEntry, OutputInfo,
};
use markdown::mdast::Node;

fn find_output_before_next_executable(children: &[Node], start: usize) -> Option<usize> {
    children[start..]
        .iter()
        .take_while(|c| !is_executable_node(c))
        .position(is_output_node)
        .map(|pos| start + pos)
}

pub fn with_output_nodes(node: &Node) -> OutputInfo {
    let mut orphans = Vec::new();

    fn process_node(node: &Node, orphans: &mut Vec<OutputEntry>) -> Node {
        if let Some(children) = node.children() {
            let (result, _) = process_children(children, orphans);
            let mut owned = node.to_owned();
            if let Some(children_mut) = owned.children_mut() {
                *children_mut = result;
            }
            owned
        } else {
            node.to_owned()
        }
    }

    fn process_children(children: &[Node], orphans: &mut Vec<OutputEntry>) -> (Vec<Node>, usize) {
        let mut result = Vec::new();
        let mut code_index: usize = 0;
        let mut i = 0;

        while i < children.len() {
            let child = &children[i];

            if is_executable_node(child) {
                let placed = process_node(child, orphans);
                result.push(placed);
                code_index += 1;

                if let Some(output_idx) = find_output_before_next_executable(children, i + 1) {
                    for j in (i + 1)..output_idx {
                        result.push(process_node(&children[j], orphans));
                    }
                    result.push(children[output_idx].to_owned());
                    i = output_idx + 1;
                } else {
                    result.push(create_output_placeholder());
                    i += 1;
                }
                continue;
            }

            if is_output_node(child) {
                if let Some(format) = output_format(child) {
                    orphans.push(OutputEntry {
                        code_index: code_index.saturating_sub(1),
                        format,
                    });
                    result.push(child.to_owned());
                }
                i += 1;
                continue;
            }

            let placed = process_node(child, orphans);
            result.push(placed);
            i += 1;
        }

        (result, code_index)
    }

    let ast = process_node(node, &mut orphans);
    OutputInfo { ast, orphans }
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
        let info = with_output_nodes(&ast);
        assert_eq!(info.ast.children().unwrap().len(), 2);
        assert!(is_output_node(&info.ast.children().unwrap()[1]));
        assert!(info.orphans.is_empty());
    }

    #[test]
    fn test_marks_orphan_output_node() {
        let input = "```output\nstale\n```";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        assert_eq!(info.ast.children().unwrap().len(), 1);
        assert_eq!(info.orphans.len(), 1);
    }

    #[test]
    fn test_keeps_valid_output_between_executables() {
        let input = "```sh exec\necho one\n```\n\n```output\none\n```\n\n```sh exec\necho two\n```";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        let children = info.ast.children().unwrap();
        assert_eq!(children.len(), 4);
        assert!(is_output_node(&children[1]));
        assert!(info.orphans.is_empty());
    }

    #[test]
    fn test_output_separated_by_text() {
        let input = "```sh exec\necho hello\n```\n\nSome text here.\n\n```output\nhello\n```";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        let children = info.ast.children().unwrap();
        assert_eq!(children.len(), 3);
        assert!(is_output_node(&children[2]));
        assert!(info.orphans.is_empty());
    }

    #[test]
    fn test_output_separated_by_heading() {
        let input = "```sh exec\necho hello\n```\n\n## Results\n\n```output\nhello\n```";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        let children = info.ast.children().unwrap();
        assert_eq!(children.len(), 3);
        assert!(is_output_node(&children[2]));
        assert!(info.orphans.is_empty());
    }

    #[test]
    fn test_output_separated_by_multiple_nodes() {
        let input = "```sh exec\necho hello\n```\n\nText\n\n## Heading\n\nMore text\n\n```output\nhello\n```";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        let children = info.ast.children().unwrap();
        assert_eq!(children.len(), 5);
        assert!(is_output_node(&children[4]));
        assert!(info.orphans.is_empty());
    }

    #[test]
    fn test_orphan_output_after_text_marked() {
        let input = "Some text\n\n```output\norphan\n```";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        let children = info.ast.children().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(info.orphans.len(), 1);
    }

    #[test]
    fn test_placeholder_added_when_no_output_in_sight() {
        let input = "```sh exec\necho hello\n```\n\nSome text after.";
        let ast = parse(input);
        let info = with_output_nodes(&ast);
        let children = info.ast.children().unwrap();
        assert_eq!(children.len(), 3);
        assert!(is_output_node(&children[1]));
        assert!(info.orphans.is_empty());
    }
}
