use std::process::Command;
use std::fs;
use std::time::Instant;
use markdown::mdast::Code;

pub const EXECUTABLE_LANGUAGES: &[&str] = &["sh", "bash", "shell", "python", "python3", "js", "javascript", "node", "ruby", "perl", "php", "go", "rust"];

#[derive(Debug, Clone)]
pub struct ExecutableCodeBlock {
    pub lang: String,
    pub code: String,
}

impl TryFrom<&Code> for ExecutableCodeBlock {
    type Error = ();

    fn try_from(code: &Code) -> Result<Self, Self::Error> {
        let lang = code.lang.as_deref().ok_or(())?;
        if is_executable(lang) {
            Ok(ExecutableCodeBlock {
                lang: lang.to_string(),
                code: code.value.clone(),
            })
        } else {
            Err(())
        }
    }
}

pub fn is_executable(lang: &str) -> bool {
    EXECUTABLE_LANGUAGES.contains(&lang)
}

#[derive(Debug)]
pub struct LanguageCommand<'a> {
    pub program: &'a str,
    pub args: &'a [&'a str],
    pub special: Option<SpecialHandler>,
}

#[derive(Debug, PartialEq)]
pub enum SpecialHandler {
    Rust,
}

pub fn get_language_command(lang: &str) -> Option<LanguageCommand<'_>> {
    match lang {
        "sh" | "bash" | "shell" => Some(LanguageCommand {
            program: "/bin/sh",
            args: &["-c"],
            special: None,
        }),
        "python" | "python3" => Some(LanguageCommand {
            program: "python3",
            args: &["-c"],
            special: None,
        }),
        "js" | "javascript" | "node" => Some(LanguageCommand {
            program: "node",
            args: &["-e"],
            special: None,
        }),
        "ruby" => Some(LanguageCommand {
            program: "ruby",
            args: &["-e"],
            special: None,
        }),
        "perl" => Some(LanguageCommand {
            program: "perl",
            args: &["-e"],
            special: None,
        }),
        "php" => Some(LanguageCommand {
            program: "php",
            args: &["-r"],
            special: None,
        }),
        "rust" => Some(LanguageCommand {
            program: "rustc",
            args: &[],
            special: Some(SpecialHandler::Rust),
        }),
        _ => None,
    }
}

pub fn execute_code_blocks(blocks: &[ExecutableCodeBlock]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            let output = execute_code(&block.lang, &block.code);
            if output.is_empty() {
                return None;
            }
            Some(output)
        })
        .collect()
}

pub fn execute_code(lang: &str, code: &str) -> String {
    let Some(config) = get_language_command(lang) else {
        return String::new();
    };

    if config.special == Some(SpecialHandler::Rust) {
        return execute_rust(code);
    }

    let output = Command::new(config.program)
        .args(config.args)
        .arg(code)
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

    #[test]
    fn test_get_language_command_shell() {
        let cmd = get_language_command("sh").unwrap();
        assert_eq!(cmd.program, "/bin/sh");
        assert_eq!(cmd.args, &["-c"]);
        assert_eq!(cmd.special, None);
    }

    #[test]
    fn test_get_language_command_python() {
        let cmd = get_language_command("python").unwrap();
        assert_eq!(cmd.program, "python3");
        assert_eq!(cmd.args, &["-c"]);
    }

    #[test]
    fn test_get_language_command_rust() {
        let cmd = get_language_command("rust").unwrap();
        assert_eq!(cmd.program, "rustc");
        assert_eq!(cmd.special, Some(SpecialHandler::Rust));
    }

    #[test]
    fn test_get_language_command_unknown() {
        assert!(get_language_command("mermaid").is_none());
    }
}

// ── Async streaming execution ──────────────────────────────────────

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    Started,
    StdoutLine(String),
    StderrLine(String),
    Completed { output: String, success: bool, duration: Duration },
}

pub fn spawn_execution_stream(
    lang: String,
    code: String,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
) {
    tokio::spawn(async move {
        let start = Instant::now();
        let _ = tx.send((index, ExecutionEvent::Started)).await;

        let Some(config) = get_language_command(&lang) else {
            let _ = tx.send((index, ExecutionEvent::Completed {
                output: String::new(),
                success: true,
                duration: start.elapsed(),
            })).await;
            return;
        };

        if config.special == Some(SpecialHandler::Rust) {
            spawn_execution_stream_rust(code, tx, index, start).await;
            return;
        }

        let mut cmd = tokio::process::Command::new(config.program);
        cmd.args(config.args);
        cmd.arg(code);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send((index, ExecutionEvent::Completed {
                    output: format!("Error: {}", e),
                    success: false,
                    duration: start.elapsed(),
                })).await;
                return;
            }
        };

        let stdout = child.stdout.expect("stdout piped");
        let stderr = child.stderr.expect("stderr piped");

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut all_output = String::new();
        let mut stdout_done = false;
        let mut stderr_done = false;

        loop {
            tokio::select! {
                result = stdout_reader.next_line(), if !stdout_done => {
                    match result {
                        Ok(Some(line)) => {
                            if !all_output.is_empty() {
                                all_output.push('\n');
                            }
                            all_output.push_str(&line);
                            let _ = tx.send((index, ExecutionEvent::StdoutLine(line))).await;
                        }
                        Ok(None) => stdout_done = true,
                        Err(_) => break,
                    }
                }
                result = stderr_reader.next_line(), if !stderr_done => {
                    match result {
                        Ok(Some(line)) => {
                            if !all_output.is_empty() {
                                all_output.push('\n');
                            }
                            all_output.push_str(&line);
                            let _ = tx.send((index, ExecutionEvent::StderrLine(line))).await;
                        }
                        Ok(None) => stderr_done = true,
                        Err(_) => break,
                    }
                }
                else => break,
            }

            if stdout_done && stderr_done {
                break;
            }
        }

        let _ = tx.send((index, ExecutionEvent::Completed {
            output: all_output.trim().to_string(),
            success: true,
            duration: start.elapsed(),
        })).await;
    });
}

async fn spawn_execution_stream_rust(
    code: String,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let temp_dir = std::env::temp_dir();
    let main_rs = temp_dir.join("main.rs");
    let output_bin = temp_dir.join("output");

    if let Err(e) = fs::write(&main_rs, &code) {
        let _ = tx.send((index, ExecutionEvent::Completed {
            output: format!("Error: Failed to write temp file: {}", e),
            success: false,
            duration: start.elapsed(),
        })).await;
        return;
    }

    let mut compile_cmd = tokio::process::Command::new("rustc");
    compile_cmd.arg("-o").arg(&output_bin).arg(&main_rs);
    compile_cmd.stdout(std::process::Stdio::piped());
    compile_cmd.stderr(std::process::Stdio::piped());

    let mut compile_child = match compile_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send((index, ExecutionEvent::Completed {
                output: format!("Error: {}", e),
                success: false,
                duration: start.elapsed(),
            })).await;
            return;
        }
    };

    let compile_stderr = compile_child.stderr.take().expect("stderr piped");
    let mut compile_reader = BufReader::new(compile_stderr).lines();

    let mut compile_output = String::new();
    while let Ok(Some(line)) = compile_reader.next_line().await {
        compile_output.push_str(&line);
        compile_output.push('\n');
        let _ = tx.send((index, ExecutionEvent::StderrLine(line))).await;
    }

    let compile_status = compile_child.wait().await;
    let _ = fs::remove_file(&main_rs);

    match compile_status {
        Ok(status) if status.success() => {
            let mut run_cmd = tokio::process::Command::new(&output_bin);
            run_cmd.stdout(std::process::Stdio::piped());
            run_cmd.stderr(std::process::Stdio::piped());

            if let Ok(run_child) = run_cmd.spawn() {
                let stdout = run_child.stdout.expect("stdout piped");
                let stderr = run_child.stderr.expect("stderr piped");

                let mut stdout_reader = BufReader::new(stdout).lines();
                let mut stderr_reader = BufReader::new(stderr).lines();

                let mut all_output = String::new();
                let mut stdout_done = false;
                let mut stderr_done = false;

                loop {
                    tokio::select! {
                        result = stdout_reader.next_line(), if !stdout_done => {
                            match result {
                                Ok(Some(line)) => {
                                    if !all_output.is_empty() {
                                        all_output.push('\n');
                                    }
                                    all_output.push_str(&line);
                                    let _ = tx.send((index, ExecutionEvent::StdoutLine(line))).await;
                                }
                                Ok(None) => stdout_done = true,
                                Err(_) => break,
                            }
                        }
                        result = stderr_reader.next_line(), if !stderr_done => {
                            match result {
                                Ok(Some(line)) => {
                                    if !all_output.is_empty() {
                                        all_output.push('\n');
                                    }
                                    all_output.push_str(&line);
                                    let _ = tx.send((index, ExecutionEvent::StderrLine(line))).await;
                                }
                                Ok(None) => stderr_done = true,
                                Err(_) => break,
                            }
                        }
                        else => break,
                    }

                    if stdout_done && stderr_done {
                        break;
                    }
                }

                let _ = tx.send((index, ExecutionEvent::Completed {
                    output: all_output.trim().to_string(),
                    success: true,
                    duration: start.elapsed(),
                })).await;
            } else {
                let _ = tx.send((index, ExecutionEvent::Completed {
                    output: "Error: Failed to run compiled binary".to_string(),
                    success: false,
                    duration: start.elapsed(),
                })).await;
            }

            let _ = fs::remove_file(&output_bin);
        }
        _ => {
            let _ = tx.send((index, ExecutionEvent::Completed {
                output: format!("Error compiling: {}", compile_output.trim()),
                success: false,
                duration: start.elapsed(),
            })).await;
        }
    }
}
