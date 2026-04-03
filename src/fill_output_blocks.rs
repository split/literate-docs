use markdown::mdast::{Code, Html, Node};

pub fn fill_output_blocks(node: &Node, outputs: &mut impl Iterator<Item = String>) -> Node {
    fn fill_children(children: &[Node], outputs: &mut impl Iterator<Item = String>) -> Vec<Node> {
        children
            .iter()
            .map(|child| fill_output_blocks(child, outputs))
            .collect()
    }

    if let Some(children) = node.children() {
        let filled = fill_children(children, outputs);
        let mut owned = node.to_owned();
        if let Some(children_mut) = owned.children_mut() {
            *children_mut = filled;
        }
        owned
    } else {
        match node {
            Node::Code(code) if code.lang.as_deref() == Some("output") => {
                if let Some(output) = outputs.next() {
                    Node::Code(Code {
                        value: output,
                        lang: Some("output".to_string()),
                        meta: None,
                        position: None,
                    })
                } else {
                    node.to_owned()
                }
            }
            Node::Html(html) if html.value.contains("<!-- output:") => {
                if let Some(output) = outputs.next() {
                    Node::Html(Html {
                        value: format!("<!-- output: {} -->", output),
                        position: None,
                    })
                } else {
                    node.to_owned()
                }
            }
            _ => node.to_owned(),
        }
    }
}
