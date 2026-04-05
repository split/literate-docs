use std::fs;
use std::time::{Duration, Instant};
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

        let Some(config) = super::execute_code_blocks::find_language(&lang) else {
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

        let Some(cmd) = config.commands.first() else {
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: "Error: No command configured".to_string(),
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
            return;
        };

        if let Some(compile) = &cmd.compile {
            spawn_compile_and_run(cmd, compile, code, tx, index, start).await;
        } else if cmd.inline {
            spawn_inline_execution(cmd, code, tx, index, start).await;
        } else {
            spawn_file_execution(cmd, code, tx, index, start).await;
        }
    });
}

async fn spawn_inline_execution(
    cmd: &super::execute_code_blocks::CommandTemplate,
    code: String,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let Some(resolved) = super::execute_code_blocks::detect_tool(cmd.tool) else {
        send_error(&tx, index, start, &format!("Error: {} not found", cmd.tool)).await;
        return;
    };

    let mut child = match tokio::process::Command::new(&resolved)
        .args(cmd.args)
        .arg(&code)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            send_error(&tx, index, start, &format!("Error: {}", e)).await;
            return;
        }
    };

    stream_output(&mut child, tx, index, start).await;
}

async fn spawn_file_execution(
    cmd: &super::execute_code_blocks::CommandTemplate,
    code: String,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let temp_dir = std::env::temp_dir();
    let input_file = temp_dir.join("main");

    if let Err(e) = fs::write(&input_file, &code) {
        send_error(
            &tx,
            index,
            start,
            &format!("Error: Failed to write temp file: {}", e),
        )
        .await;
        return;
    }

    let Some(resolved) = super::execute_code_blocks::detect_tool(cmd.tool) else {
        let _ = fs::remove_file(&input_file);
        send_error(&tx, index, start, &format!("Error: {} not found", cmd.tool)).await;
        return;
    };

    let args: Vec<String> = cmd
        .args
        .iter()
        .map(|a| resolve_arg(a, &temp_dir, &input_file))
        .collect();

    let mut child = match tokio::process::Command::new(&resolved)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&input_file);
            send_error(&tx, index, start, &format!("Error: {}", e)).await;
            return;
        }
    };

    let _ = fs::remove_file(&input_file);
    stream_output(&mut child, tx, index, start).await;
}

async fn spawn_compile_and_run(
    cmd: &super::execute_code_blocks::CommandTemplate,
    compile: &super::execute_code_blocks::CompileStep,
    code: String,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let Some(compile_resolved) = super::execute_code_blocks::detect_tool(compile.tool) else {
        send_error(
            &tx,
            index,
            start,
            &format!("Error: {} not found", compile.tool),
        )
        .await;
        return;
    };

    let temp_dir = std::env::temp_dir();
    let input_ext = get_input_extension(compile.tool);
    let input_file = temp_dir.join("main").with_extension(input_ext);
    let output_file = temp_dir.join("output");

    if let Err(e) = fs::write(&input_file, &code) {
        send_error(
            &tx,
            index,
            start,
            &format!("Error: Failed to write temp file: {}", e),
        )
        .await;
        return;
    }

    let compile_args: Vec<String> = compile
        .args
        .iter()
        .map(|a| resolve_arg_compile(a, &temp_dir, &input_file, &output_file))
        .collect();

    let mut compile_child = match tokio::process::Command::new(&compile_resolved)
        .args(&compile_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&input_file);
            send_error(&tx, index, start, &format!("Error: {}", e)).await;
            return;
        }
    };

    let compile_stderr = compile_child.stderr.take().expect("stderr piped");
    let compile_stdout = compile_child.stdout.take().expect("stdout piped");
    let mut stderr_reader = BufReader::new(compile_stderr).lines();
    let mut stdout_reader = BufReader::new(compile_stdout).lines();

    let mut compile_output = String::new();
    let mut stderr_done = false;
    let mut stdout_done = false;

    loop {
        tokio::select! {
            result = stderr_reader.next_line(), if !stderr_done => {
                match result {
                    Ok(Some(line)) => {
                        compile_output.push_str(&line);
                        compile_output.push('\n');
                        let _ = tx.send((index, ExecutionEvent::StderrLine(line))).await;
                    }
                    Ok(None) => stderr_done = true,
                    Err(_) => break,
                }
            }
            result = stdout_reader.next_line(), if !stdout_done => {
                match result {
                    Ok(Some(line)) => {
                        compile_output.push_str(&line);
                        compile_output.push('\n');
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
    let _ = fs::remove_file(&input_file);

    match compile_status {
        Ok(status) if status.success() => {
            let tool_to_run = if cmd.inline {
                if compile.tool.contains("rustc") {
                    output_file.to_string_lossy().to_string()
                } else {
                    return;
                }
            } else {
                resolve_arg_compile(
                    cmd.args.first().unwrap_or(&""),
                    &temp_dir,
                    &input_file,
                    &output_file,
                )
            };

            if tool_to_run.is_empty() {
                send_error(&tx, index, start, "Error: No tool to run after compilation").await;
                return;
            }

            let run_args: Vec<String> = cmd
                .args
                .iter()
                .skip(1)
                .map(|a| resolve_arg_compile(a, &temp_dir, &input_file, &output_file))
                .collect();

            let mut run_child = match tokio::process::Command::new(&tool_to_run)
                .args(&run_args)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = fs::remove_file(&output_file);
                    send_error(&tx, index, start, &format!("Error: {}", e)).await;
                    return;
                }
            };

            let _ = fs::remove_file(&output_file);
            stream_output(&mut run_child, tx, index, start).await;
        }
        _ => {
            let _ = fs::remove_file(&output_file);
            let error_msg = if compile_output.contains("error") {
                compile_output.trim().to_string()
            } else {
                format!("Compile error: {}", compile_output.trim())
            };
            let _ = tx
                .send((
                    index,
                    ExecutionEvent::Completed {
                        output: error_msg,
                        success: false,
                        duration: start.elapsed(),
                    },
                ))
                .await;
        }
    }
}

async fn stream_output(
    child: &mut tokio::process::Child,
    tx: mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

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
}

async fn send_error(
    tx: &mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
    msg: &str,
) {
    let _ = tx
        .send((
            index,
            ExecutionEvent::Completed {
                output: msg.to_string(),
                success: false,
                duration: start.elapsed(),
            },
        ))
        .await;
}

fn resolve_arg(arg: &str, temp_dir: &std::path::Path, input_file: &std::path::Path) -> String {
    match arg {
        "{input}" => input_file.to_string_lossy().to_string(),
        "{dir}" => temp_dir.to_string_lossy().to_string(),
        _ => arg.to_string(),
    }
}

fn resolve_arg_compile(
    arg: &str,
    temp_dir: &std::path::Path,
    input_file: &std::path::Path,
    output_file: &std::path::Path,
) -> String {
    match arg {
        "{input}" => input_file.to_string_lossy().to_string(),
        "{output}" => output_file.to_string_lossy().to_string(),
        "{dir}" => temp_dir.to_string_lossy().to_string(),
        _ => arg.to_string(),
    }
}

fn get_input_extension(compile_tool: &str) -> &str {
    if compile_tool.contains("rustc") {
        "rs"
    } else if compile_tool.contains("tsc") {
        "ts"
    } else {
        ""
    }
}
