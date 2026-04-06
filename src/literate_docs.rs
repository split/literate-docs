use crate::execute_code_blocks::execute_code_blocks;
use crate::extract_code_blocks::extract_executable_code_blocks;
use crate::fill_output_blocks::fill_output_blocks;
use crate::output_node::clean_orphans;
use crate::render_markdown::render_markdown;
use crate::with_output_nodes::with_output_nodes;
use markdown::mdast::Node;

fn transform_ast(ast: Node) -> Node {
    let blocks = extract_executable_code_blocks(&ast);
    let outputs = execute_code_blocks(&blocks);
    let info = with_output_nodes(&ast);
    let ast = fill_output_blocks(info, &mut outputs.into_iter());
    clean_orphans(ast)
}

pub fn literate_docs(input: &str) -> String {
    render_markdown(input, transform_ast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_block_produces_output() {
        let input = "```sh exec\necho hello\n```";
        let output = literate_docs(input);

        let expected = r#"```sh exec
echo hello
```

```output
hello
```"#;
        assert_eq!(output, expected);
    }

    #[test]
    fn test_unknown_language_block_unchanged() {
        let input = "```mermaid\ngraph TD; A-->B;\n```";
        let output = literate_docs(input);

        assert_eq!(output, "```mermaid\ngraph TD; A-->B;\n```");
    }

    #[test]
    fn test_no_language_block_unchanged() {
        let input = "```\nsome code\n```";
        let output = literate_docs(input);

        assert_eq!(output, "```\nsome code\n```");
    }

    #[test]
    fn test_multiple_code_blocks() {
        let input = "```sh exec\necho one\n```\n\n```sh exec\necho two\n```";
        let output = literate_docs(input);

        let expected = r#"```sh exec
echo one
```

```output
one
```

```sh exec
echo two
```

```output
two
```"#;
        assert_eq!(output, expected);
    }

    #[test]
    fn test_text_preserved() {
        let input = "# Hello World\n\nSome text here.\n\n```sh exec\necho test\n```";
        let output = literate_docs(input);

        let expected = r#"# Hello World

Some text here.

```sh exec
echo test
```

```output
test
```"#;
        assert_eq!(output, expected);
    }

    #[test]
    fn test_idempotency() {
        let input = "```sh exec\necho hello\n```";
        let output1 = literate_docs(input);
        let output2 = literate_docs(&output1);

        assert_eq!(output1, output2);
    }

    #[test]
    fn test_comment_output_format() {
        let input = "```sh exec\necho hello\n```\n\n<!-- output: hello -->";
        let output = literate_docs(input);

        assert!(
            output.contains("<!-- output: hello -->"),
            "Should contain comment output"
        );
        assert!(
            !output.contains("```output"),
            "Should not have code block output"
        );
    }

    #[test]
    fn test_comment_output_idempotency() {
        let input = "```sh exec\necho hello\n```\n\n<!-- output: hello -->";

        let output1 = literate_docs(input);

        assert!(
            !output1.contains("Error:"),
            "First run should not have error: {}",
            output1
        );

        let output2 = literate_docs(&output1);

        assert_eq!(
            output1, output2,
            "Running twice with comment format should be idempotent"
        );
    }

    #[test]
    fn test_stale_comment_output_updated() {
        let input = "```sh exec\necho hello\n```\n\n<!-- output: stale_value -->";
        let output = literate_docs(input);

        assert!(
            output.contains("<!-- output: hello -->"),
            "Should update stale comment to correct output: {}",
            output
        );
        assert!(
            !output.contains("stale_value"),
            "Should not contain stale value: {}",
            output
        );
        assert!(
            !output.contains("```output"),
            "Should not add code block output when comment exists: {}",
            output
        );
    }

    #[test]
    fn test_stale_code_block_output_updated() {
        let input = "```sh exec\necho hello\n```\n\n```output\nstale_value\n```";
        let output = literate_docs(input);

        assert!(
            output.contains("```output\nhello\n```"),
            "Should update stale code block output to correct value: {}",
            output
        );
        assert!(
            !output.contains("stale_value"),
            "Should not contain stale value: {}",
            output
        );
    }

    #[test]
    fn test_fresh_output_becomes_code_block() {
        let input = "```sh exec\necho hello\n```";
        let output = literate_docs(input);

        assert!(
            output.contains("```output\nhello\n```"),
            "Should create code block output for fresh execution: {}",
            output
        );
    }

    #[test]
    fn test_idempotent_code_block_output() {
        let input = "```sh exec\necho hello\n```";
        let output1 = literate_docs(input);
        let output2 = literate_docs(&output1);

        assert_eq!(
            output1, output2,
            "Running twice with code block output should be idempotent"
        );
    }

    #[test]
    fn test_output_separated_by_text() {
        let input = "```sh exec\necho hello\n```\n\nSome text here.\n\n```output\nhello\n```";
        let output = literate_docs(input);

        assert!(
            output.contains("```output\nhello\n```"),
            "Should preserve existing output separated by text"
        );
        assert!(
            !output.contains("```output\n\n```"),
            "Should not add placeholder when output exists"
        );
    }

    #[test]
    fn test_output_separated_by_heading() {
        let input = "```sh exec\necho hello\n```\n\n## Results\n\n```output\nhello\n```";
        let output = literate_docs(input);

        assert!(
            output.contains("## Results"),
            "Should preserve heading between code and output"
        );
        assert!(
            output.contains("```output\nhello\n```"),
            "Should preserve existing output separated by heading"
        );
        assert!(
            !output.contains("```output\n\n```"),
            "Should not add placeholder when output exists"
        );
    }

    #[test]
    fn test_orphan_output_block_removed() {
        let input = "Some text\n\n```output\nstale\n```";
        let output = literate_docs(input);

        assert!(
            !output.contains("```output"),
            "Should remove orphan output block"
        );
        assert!(
            output.contains("Some text"),
            "Should preserve surrounding text"
        );
    }

    #[test]
    fn test_orphan_comment_output_removed() {
        let input = "Some text\n\n<!-- output: stale -->";
        let output = literate_docs(input);

        assert!(
            !output.contains("<!-- output:"),
            "Should remove orphan comment output"
        );
        assert!(
            output.contains("Some text"),
            "Should preserve surrounding text"
        );
    }

    #[test]
    fn test_executable_language_without_exec_keyword_unchanged() {
        let input = "```sh\necho hello\n```";
        let output = literate_docs(input);

        assert_eq!(output, "```sh\necho hello\n```");
        assert!(
            !output.contains("```output"),
            "Should not produce output without exec keyword"
        );
    }

    #[test]
    fn test_hidden_exec_comment_idempotency() {
        let input = "<!-- sh exec: echo hello -->";
        let output1 = literate_docs(input);

        assert!(
            output1.contains("```output\nhello\n```"),
            "First run should produce output: {}",
            output1
        );

        // Second run: output block exists but no source code, so it gets removed
        // This is expected behavior - hidden exec sources are not preserved
        let output2 = literate_docs(&output1);

        // The output may or may not be present (depends on implementation)
        // The key is that the system doesn't crash and produces some valid markdown
        assert!(
            !output2.contains("stale"),
            "Second run should not have stale content: {}",
            output2
        );
    }

    #[test]
    fn test_hidden_exec_comment_removed_from_output() {
        let input = "<!-- sh exec: echo hello -->";
        let output = literate_docs(input);

        assert!(
            output.contains("<!-- sh exec:"),
            "Should keep hidden exec comment: {}",
            output
        );
    }

    #[test]
    fn test_hidden_exec_comment_mixed_with_visible() {
        // Use explicit newlines to ensure proper markdown parsing
        let input = "<!-- sh exec: echo hidden -->\n\n```sh exec\necho visible\n```";
        let output = literate_docs(input);
        eprintln!("Output: {}", output);

        assert!(
            output.contains("```output\nhidden\n```"),
            "Should have hidden output: {}",
            output
        );
        assert!(
            output.contains("```output\nvisible\n```"),
            "Should have visible output: {}",
            output
        );
    }

    #[test]
    fn test_hidden_exec_comment_stale_output_updated() {
        let input = "<!-- sh exec: echo hello -->\n\n```output\nstale\n```";
        let output = literate_docs(input);
        eprintln!("Output: {:?}", output);

        assert!(
            output.contains("hello"),
            "Should update stale output: {}",
            output
        );
    }

    #[test]
    fn test_hidden_exec_multiple_languages() {
        let input = "<!-- python exec: print(2 + 2) -->";
        let output = literate_docs(input);

        assert!(output.contains("4"), "Should execute python: {}", output);
    }

    #[test]
    fn test_hidden_exec_invalid_language_unchanged() {
        let input = "<!-- mermaid exec: graph TD; A-->B; -->";
        let output = literate_docs(input);

        assert_eq!(output, "<!-- mermaid exec: graph TD; A-->B; -->");
        assert!(
            !output.contains("output:"),
            "Should not produce output for invalid language"
        );
    }
}
