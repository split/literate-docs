use pulldown_cmark::{Parser, Event, Tag, TagEnd, CodeBlockKind, CodeBlockKind::Fenced};
use std::process::Command;
use std::fs;

fn is_output_code_block(events: &[Event], after_idx: usize) -> bool {
    for event in events.iter().skip(after_idx + 1) {
        match event {
            Event::Start(Tag::CodeBlock(Fenced(info))) => {
                let lang = info.split_whitespace().next().unwrap_or("");
                return lang == "output";
            }
            Event::Text(_) => continue,
            Event::End(TagEnd::CodeBlock) => continue,
            _ => break,
        }
    }
    false
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
            if let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event {
                let lang = info.split_whitespace().next().unwrap_or("");
                if lang == "output" {
                    skip_depth += 1;
                }
            }
            if let Event::End(TagEnd::CodeBlock) = event {
                skip_depth -= 1;
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
                    
                    // Look ahead: if there's already an output block after this one
                    let existing_output = is_output_code_block(&all_events, idx);
                    
                    events.push(Event::Text(code.into()));
                    events.push(event.clone());
                    
                    // Always add our output (this will replace any existing)
                    if !output.is_empty() {
                        events.push(Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced("output".into()))));
                        events.push(Event::Text(output.into()));
                        events.push(Event::End(TagEnd::CodeBlock));
                    }
                    
                    in_fenced_code = false;
                    
                    // If there's an existing output block after us, skip it to avoid duplication
                    if existing_output {
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
            return format!("Error: Unknown language '{}'", lang);
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
    use pulldown_cmark_to_cmark::cmark;
    
    let parser = Parser::new(input);
    let events = process_events(parser);
    
    let mut output = String::new();
    cmark(events.iter(), &mut output).ok();
    
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
    fn test_unknown_language_returns_error() {
        let result = execute_code("mermaid", "graph TD; A-->B;");
        assert!(result.contains("Unknown language"));
    }

    #[test]
    fn test_shell_block_produces_output() {
        let input = "```sh\necho hello\n```";
        let output = render_markdown(input);
        assert!(output.contains("echo hello"));
        assert!(output.contains("```output"));
        assert!(output.contains("hello"));
    }

    #[test]
    fn test_unknown_language_block_unchanged() {
        let input = "```mermaid\ngraph TD; A-->B;\n```";
        let output = render_markdown(input);
        assert!(output.contains("mermaid"));
        assert!(!output.contains("output"));
    }

    #[test]
    fn test_no_language_block_unchanged() {
        let input = "```\nsome code\n```";
        let output = render_markdown(input);
        assert!(output.contains("some code"));
    }

    #[test]
    fn test_multiple_code_blocks() {
        let input = "```sh\necho one\n```\n\n```sh\necho two\n```";
        let output = render_markdown(input);
        assert!(output.contains("one"));
        assert!(output.contains("two"));
    }

    #[test]
    fn test_text_preserved() {
        let input = "# Hello World\n\nSome text here.\n\n```sh\necho test\n```";
        let output = render_markdown(input);
        assert!(output.contains("# Hello World"));
        assert!(output.contains("Some text here"));
    }

    #[test]
    fn test_idempotency() {
        let input = "```sh\necho hello\n```";
        let output1 = render_markdown(input);
        let output2 = render_markdown(&output1);
        
        // Should not add duplicate output blocks - output2 should have same number of code blocks
        let code_blocks1: Vec<&str> = output1.lines().filter(|l| l.trim().starts_with("```")).collect();
        let code_blocks2: Vec<&str> = output2.lines().filter(|l| l.trim().starts_with("```")).collect();
        
        assert_eq!(code_blocks1.len(), code_blocks2.len(), 
            "Code block count should be same after second run.\noutput1: {:?}\noutput2: {:?}", 
            code_blocks1, code_blocks2);
    }
}