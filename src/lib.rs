use std::process::Command;
use std::fs;
use markdown::{to_mdast, ParseOptions};
use markdown::mdast::{Node, Code};
use mdast_util_to_markdown::to_markdown;

const EXECUTABLE_LANGUAGES: &[&str] = &["sh", "bash", "shell", "python", "python3", "js", "javascript", "node", "ruby", "perl", "php", "go", "rust"];

fn transform_markdown(ast: Node) -> Node {
    let mut ast = ast;
    
    fn collect_insertions(children: &[Node]) -> Vec<(usize, Node)> {
        let mut insertions = Vec::new();
        
        let mut i = 0;
        while i < children.len() {
            if let Node::Code(code) = &children[i] {
                let lang = code.lang.as_deref().unwrap_or("");
                
                // Skip output blocks and re-execution
                if lang == "output" {
                    i += 1;
                    continue;
                }
                
                if EXECUTABLE_LANGUAGES.contains(&lang) {
                    let output = execute_code(lang, &code.value);
                    
                    // Check if there's an existing HTML comment output after this code block
                    let has_comment_output = if i + 1 < children.len() {
                        matches!(&children[i + 1], Node::Html(h) if h.value.contains("<!-- output:"))
                    } else {
                        false
                    };
                    
                    if !output.is_empty() && !has_comment_output {
                        // Check if there's already a code block output
                        let has_code_output = if i + 1 < children.len() {
                            matches!(&children[i + 1], Node::Code(c) if c.lang.as_deref() == Some("output"))
                        } else {
                            false
                        };
                        
                        if !has_code_output {
                            insertions.push((i, Node::Code(Code {
                                value: output,
                                lang: Some("output".to_string()),
                                meta: None,
                                position: None,
                            })));
                        }
                    }
                }
            } else if let Node::Html(html) = &children[i] {
                // Skip HTML comment outputs to avoid duplication
                if html.value.contains("<!-- output:") {
                    i += 1;
                    continue;
                }
            }
            i += 1;
        }
        
        insertions
    }
    
    fn walk(node: &mut Node) {
        if let Some(children) = node.children_mut() {
            let insertions = collect_insertions(children);
            
            for (idx, new_node) in insertions.into_iter().enumerate() {
                children.insert(new_node.0 + idx + 1, new_node.1);
            }
            
            for child in children.iter_mut() {
                walk(child);
            }
        }
    }
    
    walk(&mut ast);
    ast
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
}
