#[cfg(test)]
mod tests {
    use crate::tui::output_box::OutputState;
    use crate::tui::render::{build_render_nodes, RenderNode, TextKind};
    use crate::tui::scroll::ScrollState;
    use crate::tui::wrap_text;
    use crate::tui::TuiApp;
    use markdown::{to_mdast, ParseOptions};

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

    // ── Line count tests ───────────────────────────────────────────

    #[test]
    fn test_heading_line_count() {
        let node = RenderNode::Text {
            content: "Short Title".to_string(),
            kind: TextKind::Heading(1),
        };
        let lines = node.line_count(80);
        assert!(lines > 0);
    }

    #[test]
    fn test_code_block_line_count() {
        let node = RenderNode::CodeBlock {
            lang: "sh".to_string(),
            code: "line1\nline2\nline3".to_string(),
        };
        let lines = node.line_count(80);
        assert_eq!(lines, 5);
    }

    #[test]
    fn test_executable_code_line_count() {
        let node = RenderNode::ExecutableCode {
            index: 0,
            lang: "sh".to_string(),
            code: "line1\nline2\nline3".to_string(),
        };
        let lines = node.line_count(80);
        assert_eq!(lines, 5);
    }

    #[test]
    fn test_pending_output_block_line_count() {
        let node = RenderNode::OutputBlock {
            code_index: 0,
            state: OutputState::Pending,
        };
        let lines = node.line_count(80);
        assert!(lines >= 3);
    }

    #[test]
    fn test_line_count_increases_with_output() {
        let short = RenderNode::OutputBlock {
            code_index: 0,
            state: OutputState::Completed {
                output: "hi".to_string(),
                previous_output: None,
                duration: std::time::Duration::from_millis(10),
                stderr: String::new(),
            },
        };
        let long = RenderNode::OutputBlock {
            code_index: 0,
            state: OutputState::Completed {
                output: "line1\nline2\nline3\nline4\nline5".to_string(),
                previous_output: None,
                duration: std::time::Duration::from_millis(10),
                stderr: String::new(),
            },
        };
        assert!(long.line_count(80) > short.line_count(80));
    }

    // ── Wrap text tests ────────────────────────────────────────────

    #[test]
    fn test_wrap_text_empty() {
        let lines = wrap_text("", 80);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_wrap_text_shorter_than_width() {
        let lines = wrap_text("hello", 80);
        assert_eq!(lines, vec!["hello"]);
    }

    #[test]
    fn test_wrap_text_splits_at_width() {
        let lines = wrap_text("1234567890", 5);
        assert_eq!(lines, vec!["12345", "67890"]);
    }

    #[test]
    fn test_wrap_text_preserves_line_breaks() {
        let lines = wrap_text("short\nalso short", 80);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_wrap_text_zero_width_returns_input() {
        let lines = wrap_text("hello", 0);
        assert_eq!(lines, vec!["hello"]);
    }

    // ── Build render nodes tests ───────────────────────────────────

    #[test]
    fn test_build_render_nodes_skips_output_blocks() {
        let input = "```sh exec\necho hi\n```\n\n```output\nhi\n```";
        let ast = to_mdast(input, &ParseOptions::default()).unwrap();
        let nodes = build_render_nodes(&ast);
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
            let nodes = build_render_nodes(&ast);
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
            let nodes = build_render_nodes(&ast);
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
        let nodes = build_render_nodes(&ast);
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
}
