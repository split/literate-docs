use std::process::Command;
use std::fs;
use markdown::{to_mdast, ParseOptions};
use markdown::mdast::{Node, Code, Html};
use mdast_util_to_markdown::to_markdown;

const EXECUTABLE_LANGUAGES: &[&str] = &["sh", "bash", "shell", "python", "python3", "js", "javascript", "node", "ruby", "perl", "php", "go", "rust"];

#[derive(Debug, Clone)]
struct CodeBlock {
    lang: Option<String>,
    value: String,
}

fn find_code_blocks(node: &Node) -> Vec<CodeBlock> {
    fn walk(node: &Node, blocks: &mut Vec<CodeBlock>) {
        if let Some(children) = node.children() {
            for child in children.iter() {
                if let Node::Code(code) = child {
                    blocks.push(CodeBlock {
                        lang: code.lang.clone(),
                        value: code.value.clone(),
                    });
                } else {
                    walk(child, blocks);
                }
            }
        }
    }
    
    let mut blocks = Vec::new();
    walk(node, &mut blocks);
    blocks
}

fn execute_blocks(blocks: &[CodeBlock]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            let lang = block.lang.as_deref().unwrap_or("");
            if lang == "output" || !EXECUTABLE_LANGUAGES.contains(&lang) {
                return None;
            }
            let output = execute_code(lang, &block.value);
            if output.is_empty() {
                return None;
            }
            Some(output)
        })
        .collect()
}

fn is_output_node(node: &Node) -> bool {
    match node {
        Node::Code(c) => c.lang.as_deref() == Some("output"),
        Node::Html(h) => h.value.contains("<!-- output:"),
        _ => false,
    }
}

fn is_executable_code(node: &Node) -> bool {
    match node {
        Node::Code(c) => {
            let lang = c.lang.as_deref().unwrap_or("");
            EXECUTABLE_LANGUAGES.contains(&lang)
        }
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

fn place_outputs(node: &Node) -> Node {
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

fn fill_outputs(node: &Node, outputs: &mut impl Iterator<Item = String>) -> Node {
    fn fill_children(children: &[Node], outputs: &mut impl Iterator<Item = String>) -> Vec<Node> {
        children.iter()
            .map(|child| fill_outputs(child, outputs))
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
            _ => node.to_owned()
        }
    }
}

fn transform_markdown(ast: Node) -> Node {
    let blocks = find_code_blocks(&ast);
    let outputs = execute_blocks(&blocks);
    let placed = place_outputs(&ast);
    fill_outputs(&placed, &mut outputs.into_iter())
}

pub fn execute_code(lang: &str, code: &str) -> String {
    let command = match lang {
        "sh" | "bash" | "shell" => ("/bin/sh", "-c", code),
        "python" | "python3" => ("python3", "-c", code),
        "js" | "javascript" | "node" => ("node", "-e", code),
        "ruby" => ("ruby", "-e", code),
        "perl" => ("perl", "-e", code),
        "php" => ("php", "-r", code),
        "go" => ("go", "run", code),
        "rust" => return execute_rust(code),
        _ => {
            return String::new();
        }
    };

    let output = Command::new(command.0)
        .arg(command.1)
        .arg(command.2)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                stdout.trim().to_string()
            } else {
                format!("Error: {}{}", stdout, stderr)
            }
        }
        Err(e) => format!("Error: {}", e),
    }
}

fn execute_rust(code: &str) -> String {
    let temp_dir = std::env::temp_dir();
    let main_rs = temp_dir.join("main.rs");
    let output = temp_dir.join("output");

    if let Err(e) = fs::write(&main_rs, code) {
        return format!("Error: Failed to write temp file: {}", e);
    }

    let compile_result = Command::new("rustc")
        .arg("-o")
        .arg(&output)
        .arg(&main_rs)
        .output();

    match compile_result {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return format!("Error compiling: {}", stderr);
            }
        }
        Err(e) => return format!("Error: {}", e),
    }

    let run_result = Command::new(&output).output();

    let _ = fs::remove_file(&main_rs);
    let _ = fs::remove_file(&output);

    match run_result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                stdout.trim().to_string()
            } else {
                format!("Error: {}{}", stdout, stderr)
            }
        }
        Err(e) => format!("Error: {}", e),
    }
}

pub fn render_markdown(input: &str) -> String {
    let has_trailing_newline = input.ends_with('\n');
    
    let ast = to_mdast(input, &ParseOptions::default())
        .expect("Failed to parse markdown");
    
    let transformed = transform_markdown(ast);
    
    let mut output = to_markdown(&transformed)
        .expect("Failed to compile markdown");
    
    if !has_trailing_newline {
        output = output.trim_end_matches('\n').to_string();
    }
    
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_execution() {
        let result = execute_code("sh", "echo hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_python_execution() {
        let result = execute_code("python", "print(2 + 2)");
        assert_eq!(result, "4");
    }

    #[test]
    fn test_node_execution() {
        let result = execute_code("node", "console.log(1 + 1)");
        assert_eq!(result, "2");
    }

    #[test]
    fn test_unknown_language_skipped() {
        let result = execute_code("mermaid", "graph TD; A-->B;");
        assert!(result.is_empty(), "Unknown language should return empty string, not error");
    }

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
