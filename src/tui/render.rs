use markdown::mdast::{Node, Text as MdText, Heading, Paragraph};
use crate::execute_code_blocks::is_executable_code_node;
use crate::with_output_nodes::{with_output_nodes, is_output_node};
use crate::tui::output_box::OutputState;

#[derive(Debug)]
pub enum RenderNode {
    Text {
        content: String,
        kind: TextKind,
    },
    CodeBlock {
        lang: String,
        code: String,
    },
    ExecutableCode {
        index: usize,
        lang: String,
        code: String,
    },
    OutputBlock {
        code_index: usize,
        state: OutputState,
    },
}

#[derive(Debug, PartialEq)]
pub enum TextKind {
    Heading(u16),
    Paragraph,
    Other,
}

impl RenderNode {
    #[cfg(test)]
    pub fn line_count(&self, terminal_width: usize) -> usize {
        match self {
            RenderNode::Text { content, kind } => {
                let prefix = match kind {
                    TextKind::Heading(level) => "#".repeat(*level as usize) + " ",
                    TextKind::Paragraph => String::new(),
                    TextKind::Other => String::new(),
                };
                let total_len = prefix.len() + content.len();
                (total_len / terminal_width).max(1)
            }
            RenderNode::CodeBlock { code, .. } => {
                code.lines().count() + 2
            }
            RenderNode::ExecutableCode { code, .. } => {
                code.lines().count() + 2
            }
            RenderNode::OutputBlock { state, .. } => {
                match state {
                    OutputState::Pending => 3,
                    OutputState::Running { live_lines, stderr_lines, .. } => {
                        live_lines.len() + if stderr_lines.is_empty() { 0 } else { stderr_lines.len() + 1 } + 2
                    }
                    OutputState::Completed { output, .. } => {
                        output.lines().count() + 2
                    }
                    OutputState::Failed { error } => {
                        error.lines().count() + 2
                    }
                }
            }
        }
    }
}

pub fn build_render_nodes(ast: &Node) -> Vec<RenderNode> {
    let placed = with_output_nodes(ast);
    let mut nodes = Vec::new();
    let mut code_index = 0;
    collect_nodes(&placed, &mut nodes, &mut code_index);
    nodes
}

fn collect_nodes(node: &Node, nodes: &mut Vec<RenderNode>, code_index: &mut usize) {
    if is_output_node(node) {
        let idx = code_index.saturating_sub(1);
        if let Node::Code(_) = node {
            nodes.push(RenderNode::OutputBlock {
                code_index: idx,
                state: OutputState::Pending,
            });
        } else if let Node::Html(h) = node {
            if h.value.contains("<!-- output:") {
                nodes.push(RenderNode::OutputBlock {
                    code_index: idx,
                    state: OutputState::Pending,
                });
            }
        }
        return;
    }

    match node {
        Node::Heading(Heading { children, depth, .. }) => {
            let text = extract_text(children);
            if !text.is_empty() {
                nodes.push(RenderNode::Text {
                    content: text,
                    kind: TextKind::Heading((*depth).into()),
                });
            }
        }
        Node::Paragraph(Paragraph { children, .. }) => {
            let text = extract_text(children);
            if !text.is_empty() {
                nodes.push(RenderNode::Text {
                    content: text,
                    kind: TextKind::Paragraph,
                });
            }
        }
        Node::Code(code) => {
            if is_executable_code_node(node) {
                nodes.push(RenderNode::ExecutableCode {
                    index: *code_index,
                    lang: code.lang.as_deref().unwrap_or("").to_string(),
                    code: code.value.clone(),
                });
                *code_index += 1;
            } else {
                nodes.push(RenderNode::CodeBlock {
                    lang: code.lang.as_deref().unwrap_or("").to_string(),
                    code: code.value.clone(),
                });
            }
        }
        _ => {}
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_nodes(child, nodes, code_index);
        }
    }
}

fn extract_text(children: &[Node]) -> String {
    children
        .iter()
        .filter_map(|c| {
            if let Node::Text(MdText { value, .. }) = c {
                Some(value.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}
