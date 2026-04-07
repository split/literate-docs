use crate::execute_code_blocks::language_config::ExecutableCodeBlock;
use crate::output_node::{extract_output_content, is_output_node, output_format, OutputEntry};
use crate::tui::output_box::OutputState;
use markdown::mdast::{
    Delete, Emphasis, Heading, InlineCode, Link, Node, Paragraph, Strong, Table, Text as MdText,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

#[derive(Debug)]
pub enum RenderNode {
    Text {
        content: Vec<Span<'static>>,
        kind: TextKind,
    },
    Table {
        rows: Vec<Vec<Vec<Span<'static>>>>,
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
        hidden: bool,
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

impl RenderNode {}

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
            let spans = extract_spans(children, Style::default());
            if !spans.is_empty() {
                nodes.push(RenderNode::Text {
                    content: spans,
                    kind: TextKind::Heading((*depth).into()),
                });
            }
        }
        Node::Paragraph(Paragraph { children, .. }) => {
            let spans = extract_spans(children, Style::default());
            if !spans.is_empty() {
                nodes.push(RenderNode::Text {
                    content: spans,
                    kind: TextKind::Paragraph,
                });
            }
        }
        _ => {}
    }

    if let Ok(block) = ExecutableCodeBlock::try_from(node) {
        nodes.push(RenderNode::ExecutableCode {
            index: *code_index,
            lang: block.lang.clone(),
            code: block.code.clone(),
            hidden: block.hidden,
        });
        *code_index += 1;
    } else if let Node::Code(code) = node {
        nodes.push(RenderNode::CodeBlock {
            lang: code.lang.as_deref().unwrap_or("").to_string(),
            code: code.value.clone(),
            executable: false,
        });
    } else if let Node::Table(Table { children, .. }) = node {
        let rows: Vec<Vec<Vec<Span<'static>>>> = children
            .iter()
            .filter_map(|row| {
                if let Node::TableRow(tr) = row {
                    let cells: Vec<Vec<Span<'static>>> = tr
                        .children
                        .iter()
                        .map(|cell| {
                            if let Node::TableCell(tc) = cell {
                                extract_spans(&tc.children, Style::default())
                            } else {
                                vec![Span::from("")]
                            }
                        })
                        .collect();
                    Some(cells)
                } else {
                    None
                }
            })
            .collect();
        if !rows.is_empty() {
            nodes.push(RenderNode::Table { rows });
        }
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_nodes(child, nodes, code_index, orphans);
        }
    }
}

pub fn extract_spans(children: &[Node], base_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for child in children {
        match child {
            Node::Text(MdText { value, .. }) => {
                if !value.is_empty() {
                    spans.push(Span::styled(value.clone(), base_style));
                }
            }
            Node::Strong(Strong { children, .. }) => {
                let mut style = base_style;
                style = style.add_modifier(Modifier::BOLD);
                spans.extend(extract_spans(children, style));
            }
            Node::Emphasis(Emphasis { children, .. }) => {
                let mut style = base_style;
                style = style.add_modifier(Modifier::ITALIC);
                spans.extend(extract_spans(children, style));
            }
            Node::InlineCode(InlineCode { value, .. }) => {
                let mut style = base_style;
                style = style.fg(Color::Green);
                spans.push(Span::styled(format!("`{}`", value), style));
            }
            Node::Delete(Delete { children, .. }) => {
                let mut style = base_style;
                style = style.add_modifier(Modifier::CROSSED_OUT);
                spans.extend(extract_spans(children, style));
            }
            Node::Link(Link { children, url, .. }) => {
                let mut style = base_style;
                style = style.fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
                let link_spans = extract_spans(children, style);
                if link_spans.is_empty() {
                    spans.push(Span::styled(url.clone(), style));
                } else {
                    spans.extend(link_spans);
                }
            }
            Node::Break(_) => {
                spans.push(Span::styled("\n", base_style));
            }
            _ => {
                if let Some(grandchildren) = child.children() {
                    spans.extend(extract_spans(grandchildren, base_style));
                }
            }
        }
    }
    spans
}
