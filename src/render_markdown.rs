use markdown::mdast::Node;
use mdast_util_to_markdown::to_markdown;
use crate::parse_markdown::parse_markdown;
use crate::extract_code_blocks::extract_code_blocks;
use crate::execute_code_blocks::execute_code_blocks;
use crate::with_output_nodes::with_output_nodes;
use crate::fill_output_blocks::fill_output_blocks;

pub fn render_markdown(input: &str) -> String {
    let has_trailing_newline = input.ends_with('\n');

    let ast = parse_markdown(input);

    let transformed = transform_markdown(ast);

    let mut output = to_markdown(&transformed)
        .expect("Failed to compile markdown");

    if !has_trailing_newline {
        output = output.trim_end_matches('\n').to_string();
    }

    output
}

fn transform_markdown(ast: Node) -> Node {
    let blocks = extract_code_blocks(&ast);
    let outputs = execute_code_blocks(&blocks);
    let placed = with_output_nodes(&ast);
    fill_output_blocks(&placed, &mut outputs.into_iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_block_produces_output() {
        let input = "```sh\necho hello\n```";
        let output = render_markdown(input);

        let expected = r#"```sh
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
        let output = render_markdown(input);

        assert_eq!(output, "```mermaid\ngraph TD; A-->B;\n```");
    }

    #[test]
    fn test_no_language_block_unchanged() {
        let input = "```\nsome code\n```";
        let output = render_markdown(input);

        assert_eq!(output, "```\nsome code\n```");
    }

    #[test]
    fn test_multiple_code_blocks() {
        let input = "```sh\necho one\n```\n\n```sh\necho two\n```";
        let output = render_markdown(input);

        let expected = r#"```sh
echo one
```

```output
one
```

```sh
echo two
```

```output
two
```"#;
        assert_eq!(output, expected);
    }

    #[test]
    fn test_text_preserved() {
        let input = "# Hello World\n\nSome text here.\n\n```sh\necho test\n```";
        let output = render_markdown(input);

        let expected = r#"# Hello World

Some text here.

```sh
echo test
```

```output
test
```"#;
        assert_eq!(output, expected);
    }

    #[test]
    fn test_idempotency() {
        let input = "```sh\necho hello\n```";
        let output1 = render_markdown(input);
        let output2 = render_markdown(&output1);

        assert_eq!(output1, output2);
    }

    #[test]
    fn test_comment_output_format() {
        let input = "```sh\necho hello\n```\n\n<!-- output: hello -->";
        let output = render_markdown(input);

        assert!(output.contains("<!-- output: hello -->"), "Should contain comment output");
        assert!(!output.contains("```output"), "Should not have code block output");
    }

    #[test]
    fn test_comment_output_idempotency() {
        let input = "```sh\necho hello\n```\n\n<!-- output: hello -->";

        let output1 = render_markdown(input);

        assert!(!output1.contains("Error:"), "First run should not have error: {}", output1);

        let output2 = render_markdown(&output1);

        assert_eq!(output1, output2, "Running twice with comment format should be idempotent");
    }

    #[test]
    fn test_stale_comment_output_updated() {
        let input = "```sh\necho hello\n```\n\n<!-- output: stale_value -->";
        let output = render_markdown(input);

        assert!(output.contains("<!-- output: hello -->"), "Should update stale comment to correct output: {}", output);
        assert!(!output.contains("stale_value"), "Should not contain stale value: {}", output);
        assert!(!output.contains("```output"), "Should not add code block output when comment exists: {}", output);
    }

    #[test]
    fn test_stale_code_block_output_updated() {
        let input = "```sh\necho hello\n```\n\n```output\nstale_value\n```";
        let output = render_markdown(input);

        assert!(output.contains("```output\nhello\n```"), "Should update stale code block output to correct value: {}", output);
        assert!(!output.contains("stale_value"), "Should not contain stale value: {}", output);
    }

    #[test]
    fn test_fresh_output_becomes_code_block() {
        let input = "```sh\necho hello\n```";
        let output = render_markdown(input);

        assert!(output.contains("```output\nhello\n```"), "Should create code block output for fresh execution: {}", output);
    }

    #[test]
    fn test_idempotent_code_block_output() {
        let input = "```sh\necho hello\n```";
        let output1 = render_markdown(input);
        let output2 = render_markdown(&output1);

        assert_eq!(output1, output2, "Running twice with code block output should be idempotent");
    }
}
