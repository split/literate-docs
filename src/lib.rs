use pulldown_cmark::{Parser, Event, Tag, TagEnd, CodeBlockKind, CodeBlockKind::Fenced};
use std::process::Command;
use std::fs;

#[derive(Debug, PartialEq, Clone, Copy)]
enum OutputFormat {
    CodeBlock,
    Comment,
}

fn detect_existing_output(events: &[Event], after_idx: usize) -> Option<OutputFormat> {
    for event in events.iter().skip(after_idx + 1) {
        match event {
            // Check for code block: ```output
            Event::Start(Tag::CodeBlock(Fenced(info))) => {
                let lang = info.split_whitespace().next().unwrap_or("");
                if lang == "output" {
                    return Some(OutputFormat::CodeBlock);
                }
                return None;
            }
            // Check for HTML comment in HtmlBlock start
            Event::Start(Tag::HtmlBlock) => {
                if let Some(Event::Html(content)) = events.get(after_idx + 2) {
                    if content.contains("<!-- output:") {
                        return Some(OutputFormat::Comment);
                    }
                }
                return None;
            }
            // Check for inline HTML comment
            Event::Html(content) => {
                if content.contains("<!-- output:") {
                    return Some(OutputFormat::Comment);
                }
                return None;
            }
            Event::Text(_) => continue,
            Event::End(TagEnd::CodeBlock) => continue,
            _ => break,
        }
    }
    None
}

pub fn process_events<'a>(parser: Parser<'a>) -> Vec<Event<'a>> {
    let mut events: Vec<Event<'a>> = Vec::new();
    let mut in_fenced_code = false;
    let mut code_content = String::new();
    let mut code_lang = String::new();
    let mut skip_depth = 0;  // How many nested output blocks to skip
    
    let all_events: Vec<_> = parser.collect();
    
    for (idx, event) in all_events.iter().enumerate() {
        // Skip events if we're in a skipped output block
        if skip_depth > 0 {
            // Handle code block output
            if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
                let lang = info.split_whitespace().next().unwrap_or("");
                if lang == "output" {
                    skip_depth += 1;
                }
            }
            if let Event::End(TagEnd::CodeBlock) = event {
                skip_depth -= 1;
            }
            // Handle comment output - decrement and check if we're done
            if let Event::Html(content) = event {
                if content.contains("<!-- output:") {
                    skip_depth -= 1;
                }
            }
            if skip_depth == 0 {
                // Done skipping, continue to next event
                continue;
            }
            continue;
        }
        
        match &event {
            Event::Text(text) => {
                if in_fenced_code {
                    code_content.push_str(text);
                } else {
                    events.push(event.clone());
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                match kind {
                    CodeBlockKind::Indented => {
                        events.push(event.clone());
                    }
                    CodeBlockKind::Fenced(info) => {
                        in_fenced_code = true;
                        code_content.clear();
                        code_lang = info.split_whitespace().next().unwrap_or("").to_string();
                        events.push(event.clone());
                    }
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if in_fenced_code {
                    let code = code_content.clone();
                    let lang = code_lang.clone();
                    
                    let is_output_block = lang == "output";
                    
                    if is_output_block {
                        if !code_content.is_empty() {
                            events.push(Event::Text(code_content.clone().into()));
                        }
                        events.push(event.clone());
                        in_fenced_code = false;
                        continue;
                    }
                    
                    let should_execute = matches!(lang.as_str(), "sh" | "bash" | "shell" | "python" | "python3" | "js" | "javascript" | "node" | "ruby" | "perl" | "php" | "go" | "rust");
                    
                    let output = if should_execute {
                        execute_code(&lang, &code)
                    } else {
                        String::new()
                    };
                    
                    // Look ahead: detect existing output format (code block or comment)
                    let existing_format = detect_existing_output(&all_events, idx);
                    
                    // Default to code block if no existing output
                    let format = existing_format.unwrap_or(OutputFormat::CodeBlock);
                    
                    events.push(Event::Text(code.into()));
                    events.push(event.clone());
                    
                    // Add output in the detected/matched format
                    if !output.is_empty() {
                        match format {
                            OutputFormat::CodeBlock => {
                                events.push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced("output".into()))));
                                events.push(Event::Text(output.into()));
                                events.push(Event::End(TagEnd::CodeBlock));
                            }
                            OutputFormat::Comment => {
                                events.push(Event::Html(format!("\n<!-- output: {} -->\n", output).into()));
                            }
                        }
                    }
                    
                    in_fenced_code = false;
                    
                    // If there's an existing output after us, skip it to avoid duplication
                    if existing_format.is_some() {
                        skip_depth = 1;
                    }
                } else {
                    events.push(event.clone());
                }
            }
            _ => {
                events.push(event.clone());
            }
        }
    }
    
    events
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
    use pulldown_cmark_to_cmark::{cmark_with_options, Options, calculate_code_block_token_count, DEFAULT_CODE_BLOCK_TOKEN_COUNT};
    
    let parser = Parser::new(input);
    let events = process_events(parser);
    let events_vec: Vec<_> = events.clone();
    
    // Calculate token count from the PROCESSED events (includes any nested output blocks)
    let token_count = calculate_code_block_token_count(events_vec.iter())
        .unwrap_or(DEFAULT_CODE_BLOCK_TOKEN_COUNT);
    
    let options = Options::<'_> { code_block_token_count: token_count, ..Default::default() };
    
    let mut output = String::new();
    cmark_with_options(events.iter(), &mut output, options).ok();
    
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
        
        let expected = r#"
```sh
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
        
        assert_eq!(output, "\n```mermaid\ngraph TD; A-->B;\n```");
    }

    #[test]
    fn test_no_language_block_unchanged() {
        let input = "```\nsome code\n```";
        let output = render_markdown(input);
        
        assert_eq!(output, "\n```\nsome code\n```");
    }

    #[test]
    fn test_multiple_code_blocks() {
        let input = "```sh\necho one\n```\n\n```sh\necho two\n```";
        let output = render_markdown(input);
        
        let expected = r#"
```sh
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
        // Input with existing comment output format should preserve it
        let input = "```sh\necho hello\n```\n\n<!-- output: hello -->";
        let output = render_markdown(input);
        
        // Should add comment output and skip existing comment
        assert!(output.contains("<!-- output: hello -->"), "Should contain comment output");
        assert!(!output.contains("```output"), "Should not have code block output");
    }

    #[test]
    fn test_comment_output_idempotency() {
        // Running twice with comment format should be idempotent
        let input = "```sh\necho hello\n```\n\n<!-- output: hello -->";
        
        // First run
        let output1 = render_markdown(input);
        
        // Check that it doesn't contain error
        assert!(!output1.contains("Error:"), "First run should not have error: {}", output1);
        
        // Second run  
        let output2 = render_markdown(&output1);
        
        // The outputs should be the same - both should have comment format and not contain errors
        assert_eq!(output1, output2, "Running twice with comment format should be idempotent");
    }
}