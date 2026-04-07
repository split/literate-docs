#[cfg(test)]
mod tests {
    use crate::tui::output_box::OutputState;
    use crate::tui::render::{build_render_nodes, extract_spans, RenderNode, TextKind};
    use crate::tui::scroll::ScrollState;
    use crate::tui::TuiApp;
    use markdown::mdast::Node;
    use markdown::{to_mdast, ParseOptions};
    use ratatui::style::Modifier;
    use ratatui::text::Span;

    fn build_nodes(ast: &markdown::mdast::Node) -> Vec<RenderNode> {
        build_render_nodes(ast, &[])
    }

    fn has_modifier(style: &ratatui::style::Style, modifier: Modifier) -> bool {
        style.add_modifier.contains(modifier)
    }

    fn spans_to_text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn extract_paragraph_children(input: &str) -> Vec<Node> {
        let ast = to_mdast(input, &ParseOptions::default()).unwrap();
        if let Node::Root(root) = &ast {
            if let Some(Node::Paragraph(p)) = root.children.first() {
                return p.children.clone();
            }
        }
        panic!("Expected paragraph in root");
    }

    // ── Parsing tests ──────────────────────────────────────────────

    #[test]
    fn test_parses_executable_blocks() {
        let input = "```sh exec\necho hi\n```\n\n```python exec\nprint(1)\n```";
        let app = TuiApp::new(input, None);
        let exec_codes: Vec<_> = app
            .nodes
            .iter()
            .filter(|n| matches!(n, RenderNode::ExecutableCode { .. }))
            .collect();
        assert_eq!(exec_codes.len(), 2);
    }

    #[test]
    fn test_parses_output_blocks() {
        let input = "```sh exec\necho hi\n```";
        let app = TuiApp::new(input, None);
        let output_blocks: Vec<_> = app
            .nodes
            .iter()
            .filter(|n| matches!(n, RenderNode::OutputBlock { .. }))
            .collect();
        assert_eq!(output_blocks.len(), 1);
    }

    #[test]
    fn test_parses_non_executable_as_code_block() {
        let input = "```mermaid\ngraph TD; A-->B;\n```";
        let app = TuiApp::new(input, None);
        assert_eq!(app.nodes.len(), 1);
        assert!(matches!(&app.nodes[0], RenderNode::CodeBlock { lang, .. } if lang == "mermaid"));
    }

    #[test]
    fn test_parses_headings() {
        let input = "# Title\n\n## Subtitle\n\nSome text";
        let app = TuiApp::new(input, None);
        let headings: Vec<_> = app
            .nodes
            .iter()
            .filter(|n| {
                matches!(
                    n,
                    RenderNode::Text {
                        kind: TextKind::Heading(_),
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(headings.len(), 2);
    }

    #[test]
    fn test_parses_paragraphs() {
        let input = "First paragraph.\n\nSecond paragraph.";
        let app = TuiApp::new(input, None);
        let paragraphs: Vec<_> = app
            .nodes
            .iter()
            .filter(|n| {
                matches!(
                    n,
                    RenderNode::Text {
                        kind: TextKind::Paragraph,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(paragraphs.len(), 2);
    }

    #[test]
    fn test_mixed_content_parses_correctly() {
        let input = "# Title\n\nSome text.\n\n```sh exec\necho hi\n```\n\nMore text.";
        let app = TuiApp::new(input, None);
        assert_eq!(app.nodes.len(), 5);
        assert!(matches!(
            &app.nodes[0],
            RenderNode::Text {
                kind: TextKind::Heading(1),
                ..
            }
        ));
        assert!(matches!(
            &app.nodes[1],
            RenderNode::Text {
                kind: TextKind::Paragraph,
                ..
            }
        ));
        assert!(matches!(&app.nodes[2], RenderNode::ExecutableCode { .. }));
        assert!(matches!(&app.nodes[3], RenderNode::OutputBlock { .. }));
        assert!(matches!(
            &app.nodes[4],
            RenderNode::Text {
                kind: TextKind::Paragraph,
                ..
            }
        ));
    }

    #[test]
    fn test_output_blocks_start_as_pending() {
        let input = "```sh exec\necho hi\n```";
        let app = TuiApp::new(input, None);
        let output_block = app
            .nodes
            .iter()
            .find(|n| matches!(n, RenderNode::OutputBlock { .. }))
            .expect("Expected OutputBlock");
        if let RenderNode::OutputBlock { state, .. } = output_block {
            assert!(matches!(state, OutputState::Pending));
        }
    }

    #[test]
    fn test_text_between_code_and_output() {
        let input = "```sh exec\necho hello\n```\n\nSome text here.\n\n```output\nhello\n```";
        let app = TuiApp::new(input, None);
        assert_eq!(app.nodes.len(), 3);
        assert!(matches!(&app.nodes[0], RenderNode::ExecutableCode { .. }));
        assert!(matches!(
            &app.nodes[1],
            RenderNode::Text {
                kind: TextKind::Paragraph,
                ..
            }
        ));
        assert!(matches!(&app.nodes[2], RenderNode::OutputBlock { .. }));
    }

    #[test]
    fn test_code_and_output_linked_by_index() {
        let input = "```sh exec\necho hi\n```";
        let app = TuiApp::new(input, None);
        let exec_code = app
            .nodes
            .iter()
            .find_map(|n| {
                if let RenderNode::ExecutableCode { index, .. } = n {
                    Some(*index)
                } else {
                    None
                }
            })
            .expect("Expected ExecutableCode");
        let output_block = app
            .nodes
            .iter()
            .find_map(|n| {
                if let RenderNode::OutputBlock { code_index, .. } = n {
                    Some(*code_index)
                } else {
                    None
                }
            })
            .expect("Expected OutputBlock");
        assert_eq!(exec_code, output_block);
    }

    // ── Scroll tests ───────────────────────────────────────────────

    #[test]
    fn test_scroll_starts_at_zero() {
        let scroll = ScrollState::new();
        assert_eq!(scroll.offset, 0);
        assert_eq!(scroll.focused_index, 0);
    }

    #[test]
    fn test_scroll_down_respects_max() {
        let mut scroll = ScrollState::new();
        scroll.scroll_down(100, 10);
        assert_eq!(scroll.offset, 10);
    }

    #[test]
    fn test_scroll_up_does_not_go_negative() {
        let mut scroll = ScrollState::new();
        scroll.offset = 5;
        scroll.scroll_up(100);
        assert_eq!(scroll.offset, 0);
    }

    #[test]
    fn test_scroll_down_by_one() {
        let mut scroll = ScrollState::new();
        scroll.scroll_down(1, 100);
        assert_eq!(scroll.offset, 1);
    }

    #[test]
    fn test_scroll_up_by_one() {
        let mut scroll = ScrollState::new();
        scroll.offset = 5;
        scroll.scroll_up(1);
        assert_eq!(scroll.offset, 4);
    }

    #[test]
    fn test_focus_next_stays_in_bounds() {
        let mut scroll = ScrollState::new();
        scroll.focus_next(3);
        scroll.focus_next(3);
        scroll.focus_next(3);
        assert_eq!(scroll.focused_index, 2);
    }

    #[test]
    fn test_focus_prev_stays_in_bounds() {
        let mut scroll = ScrollState::new();
        scroll.focused_index = 5;
        scroll.focus_prev();
        scroll.focus_prev();
        scroll.focus_prev();
        scroll.focus_prev();
        scroll.focus_prev();
        scroll.focus_prev();
        assert_eq!(scroll.focused_index, 0);
    }

    // ── Extract spans tests ────────────────────────────────────────

    #[test]
    fn test_extract_plain_text() {
        let children = extract_paragraph_children("Hello world");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        assert_eq!(spans_to_text(&spans), "Hello world");
    }

    #[test]
    fn test_extract_bold() {
        let children = extract_paragraph_children("**bold text**");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        assert_eq!(spans_to_text(&spans), "bold text");
        assert!(has_modifier(&spans[0].style, Modifier::BOLD));
    }

    #[test]
    fn test_extract_italic() {
        let children = extract_paragraph_children("*italic text*");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        assert_eq!(spans_to_text(&spans), "italic text");
        assert!(has_modifier(&spans[0].style, Modifier::ITALIC));
    }

    #[test]
    fn test_extract_inline_code() {
        let children = extract_paragraph_children("use `foo` here");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        assert_eq!(spans_to_text(&spans), "use `foo` here");
        assert_eq!(spans[1].style.fg, Some(ratatui::style::Color::Green));
    }

    #[test]
    fn test_extract_strikethrough() {
        let mut opts = ParseOptions::default();
        opts.constructs.gfm_strikethrough = true;
        let ast = to_mdast("~~deleted~~", &opts).unwrap();
        if let Node::Root(root) = &ast {
            if let Some(Node::Paragraph(p)) = root.children.first() {
                let spans = extract_spans(&p.children, ratatui::style::Style::default());
                assert_eq!(spans_to_text(&spans), "deleted");
                assert!(has_modifier(&spans[0].style, Modifier::CROSSED_OUT));
                return;
            }
        }
        panic!("Expected paragraph with strikethrough");
    }

    #[test]
    fn test_extract_link() {
        let children = extract_paragraph_children("[click here](https://example.com)");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        assert_eq!(spans_to_text(&spans), "click here");
        assert!(has_modifier(&spans[0].style, Modifier::UNDERLINED));
    }

    #[test]
    fn test_extract_nested_bold_italic() {
        let children = extract_paragraph_children("**_bold italic_**");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        assert_eq!(spans_to_text(&spans), "bold italic");
        assert!(has_modifier(&spans[0].style, Modifier::BOLD));
        assert!(has_modifier(&spans[0].style, Modifier::ITALIC));
    }

    #[test]
    fn test_extract_mixed_inline() {
        let children = extract_paragraph_children("Use **`bold code`** and *italic* here");
        let spans = extract_spans(&children, ratatui::style::Style::default());
        let text = spans_to_text(&spans);
        assert!(text.contains("bold code"));
        assert!(text.contains("italic"));
        let bold_code_span = spans
            .iter()
            .find(|s| s.content.contains("bold code"))
            .unwrap();
        assert!(has_modifier(&bold_code_span.style, Modifier::BOLD));
        assert_eq!(bold_code_span.style.fg, Some(ratatui::style::Color::Green));
    }

    // ── Build render nodes tests ───────────────────────────────────

    #[test]
    fn test_build_render_nodes_skips_output_blocks() {
        let input = "```sh exec\necho hi\n```\n\n```output\nhi\n```";
        let ast = to_mdast(input, &ParseOptions::default()).unwrap();
        let nodes = build_nodes(&ast);
        let exec: Vec<_> = nodes
            .iter()
            .filter(|n| matches!(n, RenderNode::ExecutableCode { .. }))
            .collect();
        let output: Vec<_> = nodes
            .iter()
            .filter(|n| matches!(n, RenderNode::OutputBlock { .. }))
            .collect();
        assert_eq!(exec.len(), 1);
        assert_eq!(output.len(), 1);
    }

    #[test]
    fn test_build_render_nodes_marks_executable_correctly() {
        let langs = ["sh", "bash", "python", "js", "node"];
        for lang in langs {
            let input = format!("```{} exec\ncode\n```", lang);
            let ast = to_mdast(&input, &ParseOptions::default()).unwrap();
            let nodes = build_nodes(&ast);
            assert!(
                nodes
                    .iter()
                    .any(|n| matches!(n, RenderNode::ExecutableCode { .. })),
                "{} should be executable",
                lang
            );
        }
    }

    #[test]
    fn test_build_render_nodes_marks_non_executable_correctly() {
        let langs = ["mermaid", "json", "yaml", "css", "html"];
        for lang in langs {
            let input = format!("```{}\ncode\n```", lang);
            let ast = to_mdast(&input, &ParseOptions::default()).unwrap();
            let nodes = build_nodes(&ast);
            assert!(
                nodes
                    .iter()
                    .any(|n| matches!(n, RenderNode::CodeBlock { .. })),
                "{} should NOT be executable",
                lang
            );
        }
    }

    #[test]
    fn test_text_between_code_and_output_preserved() {
        let input = "```sh exec\necho hello\n```\n\nText between.\n\n```output\nhello\n```";
        let ast = to_mdast(input, &ParseOptions::default()).unwrap();
        let nodes = build_nodes(&ast);
        assert!(nodes
            .iter()
            .any(|n| matches!(n, RenderNode::ExecutableCode { .. })));
        assert!(nodes.iter().any(|n| matches!(
            n,
            RenderNode::Text {
                kind: TextKind::Paragraph,
                ..
            }
        )));
        assert!(nodes
            .iter()
            .any(|n| matches!(n, RenderNode::OutputBlock { .. })));
    }

    #[test]
    fn test_executable_and_output_nodes_created() {
        let input = "```sh exec\necho hello\n```";
        let app = TuiApp::new(input, None);
        let has_exec = app
            .nodes
            .iter()
            .any(|n| matches!(n, RenderNode::ExecutableCode { .. }));
        let has_output = app.nodes.iter().any(|n| {
            matches!(n, RenderNode::OutputBlock { state, .. } if matches!(state, OutputState::Pending))
        });
        assert!(has_exec, "Should have ExecutableCode node");
        assert!(has_output, "Should have OutputBlock node in Pending state");
    }
}
