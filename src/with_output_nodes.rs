use markdown::mdast::{Node, Code};
use crate::execute_code_blocks::is_executable_code;

pub fn is_output_node(node: &Node) -> bool {
    match node {
        Node::Code(c) => c.lang.as_deref() == Some("output"),
        Node::Html(h) => h.value.contains("<!-- output:"),
        _ => false,
    }
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
    fn place_node(node: &Node) -> Node {
        if let Some(children) = node.children() {
            let mut result = Vec::new();
            let mut i = 0;
            while i < children.len() {
                let child = &children[i];

                if is_output_node(child) {
                    result.push(child.to_owned());
                    i += 1;
                    continue;
                }

                let placed = place_node(child);

                if is_executable_code(child) {
                    let has_output = if i + 1 < children.len() {
                        is_output_node(&children[i + 1])
                    } else {
                        false
                    };

                    result.push(placed);
                    if !has_output {
                        result.push(create_empty_output_placeholder());
                    }
                } else {
                    result.push(placed);
                }

                i += 1;
            }

            let mut owned = node.to_owned();
            if let Some(children_mut) = owned.children_mut() {
                *children_mut = result;
            }
            owned
        } else {
            node.to_owned()
        }
    }

    place_node(node)
}
