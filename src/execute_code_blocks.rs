use markdown::mdast::Node;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub const EXECUTABLE_LANGUAGES: &[&str] = &[
    "sh",
    "bash",
    "shell",
    "python",
    "python3",
    "js",
    "javascript",
    "node",
    "ts",
    "typescript",
    "ruby",
    "perl",
    "php",
    "go",
    "rust",
];

#[derive(Debug, Clone)]
pub struct ExecutableCodeBlock {
    pub lang: String,
    pub code: String,
    pub hidden: bool,
}

impl TryFrom<&Node> for ExecutableCodeBlock {
    type Error = ();

    fn try_from(node: &Node) -> Result<Self, Self::Error> {
        match node {
            Node::Code(code) => {
                let lang = code.lang.as_deref().ok_or(())?;
                if is_executable(lang)
                    && code
                        .meta
                        .as_deref()
                        .map(|m| m.split_whitespace().any(|t| t == "exec"))
                        .unwrap_or(false)
                {
                    Ok(ExecutableCodeBlock {
                        lang: lang.to_string(),
                        code: code.value.clone(),
                        hidden: false,
                    })
                } else {
                    Err(())
                }
            }
            Node::Html(html) => {
                if let Some((lang, code)) = parse_hidden_exec_comment(&html.value) {
                    Ok(ExecutableCodeBlock {
                        lang,
                        code,
                        hidden: true,
                    })
                } else {
                    Err(())
                }
            }
            _ => Err(()),
        }
    }
}

pub fn is_executable_code_node(node: &Node) -> bool {
    match node {
        Node::Code(c) => {
            let lang = c.lang.as_deref().unwrap_or("");
            is_executable(lang)
                && c.meta
                    .as_deref()
                    .map(|m| m.split_whitespace().any(|t| t == "exec"))
                    .unwrap_or(false)
        }
        _ => false,
    }
}

pub fn is_executable_node(node: &Node) -> bool {
    is_executable_code_node(node) || is_hidden_executable_comment(node).is_some()
}

pub fn is_hidden_executable_comment(node: &Node) -> Option<(String, String)> {
    match node {
        Node::Html(h) => parse_hidden_exec_comment(&h.value),
        _ => None,
    }
}

fn parse_hidden_exec_comment(comment: &str) -> Option<(String, String)> {
    let trimmed = comment.trim();
    if !trimmed.starts_with("<!--") || !trimmed.ends_with("-->") {
        return None;
    }
    let inner = &trimmed[4..trimmed.len() - 3].trim();
    let parts: Vec<&str> = inner.splitn(2, " exec: ").collect();
    if parts.len() != 2 {
        return None;
    }
    let lang = parts[0].trim();
    let code = parts[1].trim();
    if lang.is_empty() || code.is_empty() {
        return None;
    }
    if !is_executable(lang) {
        return None;
    }
    Some((lang.to_string(), code.to_string()))
}

pub fn is_executable(lang: &str) -> bool {
    LANGUAGES
        .iter()
        .any(|config| config.aliases.contains(&lang))
}

struct CommandTemplate {
    tool: &'static str,
    args: &'static [&'static str],
    inline: bool,
    run_after: Option<(&'static str, &'static [&'static str])>,
}

struct LanguageConfig {
    aliases: &'static [&'static str],
    commands: &'static [CommandTemplate],
}

const LANGUAGES: &[LanguageConfig] = &[
    LanguageConfig {
        aliases: &["sh", "bash", "shell"],
        commands: &[CommandTemplate {
            tool: "/bin/sh",
            args: &["-c"],
            inline: true,
            run_after: None,
        }],
    },
    LanguageConfig {
        aliases: &["python", "python3"],
        commands: &[CommandTemplate {
            tool: "python3",
            args: &["-c"],
            inline: true,
            run_after: None,
        }],
    },
    LanguageConfig {
        aliases: &["js", "javascript", "node"],
        commands: &[
            CommandTemplate {
                tool: "node_modules/.bin/node",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "node_modules/.bin/bun",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "node",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "bun",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
        ],
    },
    LanguageConfig {
        aliases: &["ruby"],
        commands: &[CommandTemplate {
            tool: "ruby",
            args: &["-e"],
            inline: true,
            run_after: None,
        }],
    },
    LanguageConfig {
        aliases: &["perl"],
        commands: &[CommandTemplate {
            tool: "perl",
            args: &["-e"],
            inline: true,
            run_after: None,
        }],
    },
    LanguageConfig {
        aliases: &["php"],
        commands: &[CommandTemplate {
            tool: "php",
            args: &["-r"],
            inline: true,
            run_after: None,
        }],
    },
    LanguageConfig {
        aliases: &["go"],
        commands: &[CommandTemplate {
            tool: "go",
            args: &["run", "{input}"],
            inline: false,
            run_after: None,
        }],
    },
    LanguageConfig {
        aliases: &["rust"],
        commands: &[CommandTemplate {
            tool: "rustc",
            args: &["-o", "{output}", "{input}"],
            inline: false,
            run_after: Some(("{output}", &[])),
        }],
    },
    LanguageConfig {
        aliases: &["ts", "typescript"],
        commands: &[
            CommandTemplate {
                tool: "node_modules/.bin/ts-node",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "node_modules/.bin/tsx",
                args: &[],
                inline: false,
                run_after: None,
            },
            CommandTemplate {
                tool: "node_modules/.bin/bun",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "node_modules/.bin/node",
                args: &["--experimental-strip-types"],
                inline: false,
                run_after: None,
            },
            CommandTemplate {
                tool: "ts-node",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "tsx",
                args: &[],
                inline: false,
                run_after: None,
            },
            CommandTemplate {
                tool: "bun",
                args: &["-e"],
                inline: true,
                run_after: None,
            },
            CommandTemplate {
                tool: "node",
                args: &["--experimental-strip-types"],
                inline: false,
                run_after: None,
            },
        ],
    },
];

fn find_language(lang: &str) -> Option<&'static LanguageConfig> {
    LANGUAGES
        .iter()
        .find(|config| config.aliases.contains(&lang))
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
    let Some(config) = find_language(lang) else {
        return String::new();
    };
    execute_language(config, code)
}

fn unique_temp_dir() -> PathBuf {
    let pid = std::process::id();
    std::env::temp_dir().join(format!("literate_{}", pid))
}

fn detect_tool(tool: &str) -> Option<PathBuf> {
    if Command::new(tool).arg("--version").output().is_ok() {
        Some(PathBuf::from(tool))
    } else {
        None
    }
}

fn resolve_arg(arg: &str, temp_dir: &Path, input_file: &Path) -> String {
    let output_file = temp_dir.join("output");
    match arg {
        "{input}" => input_file.to_string_lossy().to_string(),
        "{output}" => output_file.to_string_lossy().to_string(),
        "{dir}" => temp_dir.to_string_lossy().to_string(),
        _ => arg.to_string(),
    }
}

fn run_inline(resolved: &Path, args: &[&str], code: &str) -> Option<String> {
    let output = Command::new(resolved).args(args).arg(code).output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn run_with_file(resolved: &Path, args: &[&str], code: &str, temp_dir: &Path) -> Option<String> {
    fs::create_dir_all(temp_dir).ok()?;

    let input_file = temp_dir.join("main");
    fs::write(&input_file, code).ok()?;

    let resolved_args: Vec<String> = args
        .iter()
        .map(|a| resolve_arg(a, temp_dir, &input_file))
        .collect();

    let output = Command::new(resolved).args(&resolved_args).output();

    let _ = fs::remove_dir_all(temp_dir);

    let out = output.ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !stderr.is_empty() || !stdout.is_empty() {
            Some(format!("{}{}", stdout, stderr).trim().to_string())
        } else {
            None
        }
    }
}

fn run_with_file_and_after(
    resolved: &Path,
    args: &[&str],
    code: &str,
    temp_dir: &Path,
    after_tool: &str,
    after_args: &[&str],
) -> Option<String> {
    fs::create_dir_all(temp_dir).ok()?;

    let input_file = temp_dir.join("main");
    fs::write(&input_file, code).ok()?;

    let resolved_args: Vec<String> = args
        .iter()
        .map(|a| resolve_arg(a, temp_dir, &input_file))
        .collect();

    let compile_output = Command::new(resolved).args(&resolved_args).output();

    let _ = fs::remove_file(&input_file);

    let compiled = compile_output.ok()?;
    if !compiled.status.success() {
        let stderr = String::from_utf8_lossy(&compiled.stderr);
        let stdout = String::from_utf8_lossy(&compiled.stdout);
        return Some(
            format!("Compile error: {}{}", stdout, stderr)
                .trim()
                .to_string(),
        );
    }

    let output_file = temp_dir.join("output");
    let run_tool = resolve_arg(after_tool, temp_dir, &input_file);
    let run_args: Vec<String> = after_args
        .iter()
        .map(|a| resolve_arg(a, temp_dir, &input_file))
        .collect();

    let run_output = Command::new(&run_tool).args(&run_args).output();

    let _ = fs::remove_file(&output_file);

    let out = run_output.ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        Some(format!("{}{}", stdout, stderr).trim().to_string())
    }
}

fn execute_language(config: &LanguageConfig, code: &str) -> String {
    for cmd in config.commands {
        let Some(resolved) = detect_tool(cmd.tool) else {
            continue;
        };

        let result = if let Some((after_tool, after_args)) = cmd.run_after {
            let temp_dir = unique_temp_dir();
            run_with_file_and_after(&resolved, cmd.args, code, &temp_dir, after_tool, after_args)
        } else if cmd.inline {
            run_inline(&resolved, cmd.args, code)
        } else {
            let temp_dir = unique_temp_dir();
            run_with_file(&resolved, cmd.args, code, &temp_dir)
        };

        if let Some(output) = result {
            return output;
        }
    }

    format!(
        "Error: No runtime found for {}. Available: {}",
        config.aliases[0],
        config
            .commands
            .iter()
            .map(|c| c.tool)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn detect_tsc() -> Option<String> {
    detect_tool("tsc").map(|p| p.to_string_lossy().to_string())
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
        assert!(
            result.is_empty(),
            "Unknown language should return empty string, not error"
        );
    }

    #[test]
    fn test_find_language_shell() {
        let lang = find_language("sh");
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().commands[0].tool, "/bin/sh");
    }

    #[test]
    fn test_find_language_python() {
        let lang = find_language("python");
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().commands[0].tool, "python3");
    }

    #[test]
    fn test_find_language_rust() {
        let lang = find_language("rust");
        assert!(lang.is_some());
        assert!(lang.unwrap().aliases.contains(&"rust"));
    }

    #[test]
    fn test_find_language_typescript() {
        let lang = find_language("ts");
        assert!(lang.is_some());
        assert!(lang.unwrap().aliases.contains(&"ts"));
        let lang2 = find_language("typescript");
        assert!(lang2.is_some());
    }

    #[test]
    fn test_typescript_execution() {
        let result = execute_code("ts", "console.log('hello from ts');");
        assert!(
            result.contains("hello from ts"),
            "Should execute TypeScript and return output, got: {}",
            result
        );
    }

    #[test]
    fn test_find_language_unknown() {
        assert!(find_language("mermaid").is_none());
    }

    #[test]
    fn test_detect_tool_global() {
        let result = detect_tool("node");
        assert!(result.is_some(), "node should be found globally");
    }

    #[test]
    fn test_detect_tool_missing() {
        let result = detect_tool("nonexistent_binary_xyz");
        assert!(result.is_none(), "nonexistent tool should not be found");
    }

    #[test]
    fn test_rust_execution() {
        let result = execute_code("rust", "fn main() { println!(\"hello from rust\"); }");
        assert!(
            result.contains("hello from rust"),
            "Should execute Rust and return output, got: {}",
            result
        );
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
    Completed {
        output: String,
        success: bool,
        duration: Duration,
    },
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

        let Some(config) = find_language(&lang) else {
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: String::new(),
                        success: true,
                        duration: start.elapsed(),
                    },
                ))
                .await;
            return;
        };

        if config.aliases.contains(&"rust") {
            spawn_execution_stream_rust(code, tx, index, start).await;
            return;
        }

        if config.aliases.contains(&"ts") || config.aliases.contains(&"typescript") {
            spawn_execution_stream_typescript(code, tx, index, start).await;
            return;
        }

        let program = config.commands[0].tool;
        let args: Vec<&str> = config.commands[0].args.iter().copied().collect();

        let mut cmd = tokio::process::Command::new(program);
        cmd.args(&args);
        cmd.arg(&code);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send((
                        index,
                        ExecutionEvent::Completed {
                            output: format!("Error: {}", e),
                            success: false,
                            duration: start.elapsed(),
                        },
                    ))
                    .await;
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

        let _ = tx
            .send((
                index,
                ExecutionEvent::Completed {
                    output: all_output.trim().to_string(),
                    success: true,
                    duration: start.elapsed(),
                },
            ))
            .await;
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
        let _ = tx
            .send((
                index,
                ExecutionEvent::Completed {
                    output: format!("Error: Failed to write temp file: {}", e),
                    success: false,
                    duration: start.elapsed(),
                },
            ))
            .await;
        return;
    }

    let mut compile_cmd = tokio::process::Command::new("rustc");
    compile_cmd.arg("-o").arg(&output_bin).arg(&main_rs);
    compile_cmd.stdout(std::process::Stdio::piped());
    compile_cmd.stderr(std::process::Stdio::piped());

    let mut compile_child = match compile_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: format!("Error: {}", e),
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
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

                let _ = tx
                    .send((
                        index,
                        ExecutionEvent::Completed {
                            output: all_output.trim().to_string(),
                            success: true,
                            duration: start.elapsed(),
                        },
                    ))
                    .await;
            } else {
                let _ = tx
                    .send((
                        index,
                        ExecutionEvent::Completed {
                            output: "Error: Failed to run compiled binary".to_string(),
                            success: false,
                            duration: start.elapsed(),
                        },
                    ))
                    .await;
            }

            let _ = fs::remove_file(&output_bin);
        }
        _ => {
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: format!("Error compiling: {}", compile_output.trim()),
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
        }
    }
}

async fn spawn_execution_stream_typescript(
    code: String,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let tsc_path = match detect_tsc() {
        Some(p) => p,
        None => {
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: "Error: tsc not found. Install TypeScript with: npm install -g typescript"
                            .to_string(),
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
            return;
        }
    };

    let temp_dir = std::env::temp_dir();
    let temp_ts = temp_dir.join("temp.ts");
    let temp_js = temp_dir.join("temp.js");

    if let Err(e) = fs::write(&temp_ts, &code) {
        let _ = tx
            .send((
                index,
                ExecutionEvent::Completed {
                    output: format!("Error: Failed to write temp file: {}", e),
                    success: false,
                    duration: start.elapsed(),
                },
            ))
            .await;
        return;
    }

    let mut compile_cmd = tokio::process::Command::new(&tsc_path);
    compile_cmd
        .arg(&temp_ts)
        .arg("--outDir")
        .arg(&temp_dir)
        .arg("--target")
        .arg("ES2020")
        .arg("--module")
        .arg("CommonJS")
        .arg("--esModuleInterop")
        .arg("--skipLibCheck");
    compile_cmd.stdout(std::process::Stdio::piped());
    compile_cmd.stderr(std::process::Stdio::piped());

    let mut compile_child = match compile_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&temp_ts);
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: format!("Error: {}", e),
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
            return;
        }
    };

    let compile_stderr = compile_child.stderr.take().expect("stderr piped");
    let compile_stdout = compile_child.stdout.take().expect("stdout piped");
    let mut stderr_reader = BufReader::new(compile_stderr).lines();
    let mut stdout_reader = BufReader::new(compile_stdout).lines();

    let mut compile_stderr_output = String::new();
    let mut compile_stdout_output = String::new();
    let mut stderr_done = false;
    let mut stdout_done = false;

    loop {
        tokio::select! {
            result = stderr_reader.next_line(), if !stderr_done => {
                match result {
                    Ok(Some(line)) => {
                        compile_stderr_output.push_str(&line);
                        compile_stderr_output.push('\n');
                        let _ = tx.send((index, ExecutionEvent::StderrLine(line))).await;
                    }
                    Ok(None) => stderr_done = true,
                    Err(_) => break,
                }
            }
            result = stdout_reader.next_line(), if !stdout_done => {
                match result {
                    Ok(Some(line)) => {
                        compile_stdout_output.push_str(&line);
                        compile_stdout_output.push('\n');
                        let _ = tx.send((index, ExecutionEvent::StdoutLine(line))).await;
                    }
                    Ok(None) => stdout_done = true,
                    Err(_) => break,
                }
            }
            else => break,
        }

        if stderr_done && stdout_done {
            break;
        }
    }

    let compile_status = compile_child.wait().await;
    let _ = fs::remove_file(&temp_ts);

    match compile_status {
        Ok(status) if status.success() => {
            let mut run_cmd = tokio::process::Command::new("node");
            run_cmd.arg(&temp_js);
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

                let _ = tx
                    .send((
                        index,
                        ExecutionEvent::Completed {
                            output: all_output.trim().to_string(),
                            success: true,
                            duration: start.elapsed(),
                        },
                    ))
                    .await;
            } else {
                let _ = fs::remove_file(&temp_js);
                let _ = tx
                    .send((
                        index,
                        ExecutionEvent::Completed {
                            output: "Error: Failed to run node".to_string(),
                            success: false,
                            duration: start.elapsed(),
                        },
                    ))
                    .await;
            }

            let _ = fs::remove_file(&temp_js);
        }
        _ => {
            let _ = fs::remove_file(&temp_js);
            let compile_error = if compile_stdout_output.contains("error") {
                compile_stdout_output.clone()
            } else {
                compile_stderr_output
            };
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: format!("TypeScript compile error:\n{}", compile_error.trim()),
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
        }
    }
}
