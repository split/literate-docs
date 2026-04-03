use std::process::Command;
use std::fs;
use markdown::mdast::Node;
use crate::extract_code_blocks::CodeBlock;

const EXECUTABLE_LANGUAGES: &[&str] = &["sh", "bash", "shell", "python", "python3", "js", "javascript", "node", "ruby", "perl", "php", "go", "rust"];

pub fn is_executable_code(node: &Node) -> bool {
    match node {
        Node::Code(c) => {
            let lang = c.lang.as_deref().unwrap_or("");
            EXECUTABLE_LANGUAGES.contains(&lang)
        }
        _ => false,
    }
}

pub fn execute_code_blocks(blocks: &[CodeBlock]) -> Vec<String> {
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
}
