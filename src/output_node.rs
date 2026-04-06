use markdown::mdast::{Code, Html, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    CodeBlock,
    HtmlComment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputEntry {
    pub code_index: usize,
    pub format: OutputFormat,
}

pub struct OutputInfo {
    pub ast: Node,
    pub orphans: Vec<OutputEntry>,
}

pub fn is_output_node(node: &Node) -> bool {
    matches!(node, Node::Code(c) if c.lang.as_deref() == Some("output"))
        || matches!(node, Node::Html(h) if h.value.contains("<!-- output:"))
}

pub fn output_format(node: &Node) -> Option<OutputFormat> {
    match node {
        Node::Code(c) if c.lang.as_deref() == Some("output") => Some(OutputFormat::CodeBlock),
        Node::Html(h) if h.value.contains("<!-- output:") => Some(OutputFormat::HtmlComment),
        _ => None,
    }
}

pub fn create_output_placeholder() -> Node {
    Node::Code(Code {
        value: String::new(),
        lang: Some("output".to_string()),
        meta: None,
        position: None,
    })
}

pub fn update_output_value(node: &mut Node, output: &str) -> bool {
    match node {
        Node::Code(code) if code.lang.as_deref() == Some("output") => {
            code.value = output.to_string();
            code.meta = None;
            code.position = None;
            true
        }
        Node::Html(html) if html.value.contains("<!-- output:") => {
            html.value = format!("<!-- output: {} -->", output);
            html.position = None;
            true
        }
        _ => false,
    }
}

pub fn extract_output_content(node: &Node) -> String {
    match node {
        Node::Code(Code { value, .. }) => value.clone(),
        Node::Html(Html { value, .. }) => value
            .strip_prefix("<!-- output: ")
            .and_then(|s| s.strip_suffix(" -->"))
            .unwrap_or(value)
            .to_string(),
        _ => String::new(),
    }
}

pub fn clean_orphans(ast: Node) -> Node {
    fn clean_node(node: &mut Node) {
        if let Some(children) = node.children_mut() {
            clean_children(children);
            for child in children.iter_mut() {
                clean_node(child);
            }
        }
    }

    fn clean_children(children: &mut Vec<Node>) {
        let mut result = Vec::new();
        let mut i = 0;
        let mut had_executable = false;

        while i < children.len() {
            let child = &children[i];

            if crate::execute_code_blocks::is_executable_node(child) {
                had_executable = true;
                result.push(children[i].to_owned());
                i += 1;

                let mut found_output = false;
                while i < children.len() {
                    if is_output_node(&children[i]) {
                        if !found_output {
                            result.push(children[i].to_owned());
                            found_output = true;
                        }
                        i += 1;
                    } else if crate::execute_code_blocks::is_executable_node(&children[i]) {
                        break;
                    } else {
                        result.push(children[i].to_owned());
                        i += 1;
                    }
                }
            } else if is_output_node(child) {
                if had_executable {
                    result.push(children[i].to_owned());
                }
                i += 1;
            } else {
                result.push(children[i].to_owned());
                i += 1;
            }
        }

        *children = result;
    }

    let mut ast = ast;
    clean_node(&mut ast);
    ast
}
