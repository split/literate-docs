use crate::execute_code_blocks::is_executable_code_node;
use crate::output_node::{extract_output_content, is_output_node, output_format, OutputEntry};
use crate::tui::output_box::OutputState;
use markdown::mdast::{Heading, Node, Paragraph, Text as MdText};

#[derive(Debug)]
pub enum RenderNode {
    Text {
        content: String,
        kind: TextKind,
    },
    CodeBlock {
        lang: String,
        code: String,
        executable: bool,
    },
    ExecutableCode {
        index: usize,
        lang: String,
        code: String,
    },
    OutputBlock {
        code_index: usize,
        state: OutputState,
        is_orphan: bool,
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
            RenderNode::CodeBlock { code, .. } => code.lines().count() + 2,
            RenderNode::ExecutableCode { code, .. } => code.lines().count() + 2,
            RenderNode::OutputBlock { state, .. } => match state {
                OutputState::Pending => 3,
                OutputState::Running {
                    live_lines,
                    stderr_lines,
                    ..
                } => {
                    live_lines.len()
                        + if stderr_lines.is_empty() {
                            0
                        } else {
                            stderr_lines.len() + 1
                        }
                        + 2
                }
                OutputState::Completed { output, .. } => output.lines().count() + 2,
                OutputState::Failed { error } => error.lines().count() + 2,
                OutputState::Orphaned { content } => content.lines().count() + 2,
            },
        }
    }
}

pub fn build_render_nodes(
    ast: &Node,
    orphans: &[crate::output_node::OutputEntry],
) -> Vec<RenderNode> {
    let mut nodes = Vec::new();
    let mut code_index = 0;
    collect_nodes(ast, &mut nodes, &mut code_index, orphans);
    nodes
}

fn collect_nodes(
    node: &Node,
    nodes: &mut Vec<RenderNode>,
    code_index: &mut usize,
    orphans: &[OutputEntry],
) {
    if is_output_node(node) {
        let idx = code_index.saturating_sub(1);
        let is_orphan = orphans
            .iter()
            .any(|o| o.code_index == idx && output_format(node) == Some(o.format));
        let state = if is_orphan {
            let content = extract_output_content(node);
            OutputState::Orphaned { content }
        } else {
            OutputState::Pending
        };
        nodes.push(RenderNode::OutputBlock {
            code_index: idx,
            state,
            is_orphan,
        });
        return;
    }

    match node {
        Node::Heading(Heading {
            children, depth, ..
        }) => {
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
            let is_executable = is_executable_code_node(node);
            if is_executable {
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
                    executable: false,
                });
            }
        }
        _ => {}
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_nodes(child, nodes, code_index, orphans);
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
