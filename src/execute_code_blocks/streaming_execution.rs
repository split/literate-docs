use std::fs;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::execute_code_blocks::default_language_config::find_language;
use crate::execute_code_blocks::language_config::{CommandTemplate, ExecCommand};
use crate::execute_code_blocks::sync_execution::{detect_tool, resolve_arg_compile};

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
            send_completed(&tx, index, start, String::new(), true).await;
            return;
        };

        let Some(cmd) = config.commands.first() else {
            send_completed(
                &tx,
                index,
                start,
                "Error: No command configured".to_string(),
                false,
            )
            .await;
            return;
        };

        if cmd.compile.is_some() {
            run_with_compile(cmd, &code, &tx, index, start).await;
        } else if cmd.run.inline {
            run_inline(&cmd.run, &code, &tx, index, start).await;
        } else {
            run_with_file(&cmd.run, &code, &tx, index, start).await;
        }
    });
}

async fn send_completed(
    tx: &mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
    output: String,
    success: bool,
) {
    let _ = tx
        .send((
            index,
            ExecutionEvent::Completed {
                output,
                success,
                duration: start.elapsed(),
            },
        ))
        .await;
}

async fn spawn_and_stream(
    cmd: &mut Command,
    tx: &mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
) -> (String, bool) {
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (format!("Error: {}", e), false),
    };

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
                        if !all_output.is_empty() { all_output.push('\n'); }
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
                        if !all_output.is_empty() { all_output.push('\n'); }
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

    match child.wait().await {
        Ok(status) => (all_output.trim().to_string(), status.success()),
        Err(e) => (format!("Error: {}", e), false),
    }
}

async fn run_inline(
    cmd: &ExecCommand,
    code: &str,
    tx: &mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let Some(resolved) = detect_tool(cmd.tool) else {
        send_completed(
            tx,
            index,
            start,
            format!("Error: {} not found", cmd.tool),
            false,
        )
        .await;
        return;
    };

    let mut c = Command::new(&resolved);
    c.args(cmd.args)
        .arg(code)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (output, success) = spawn_and_stream(&mut c, tx, index).await;
    send_completed(tx, index, start, output, success).await;
}

async fn run_with_file(
    cmd: &ExecCommand,
    code: &str,
    tx: &mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let temp_dir = std::env::temp_dir();
    let input_file = temp_dir.join("main");
    let output_file = temp_dir.join("output");

    if let Err(e) = fs::write(&input_file, code) {
        send_completed(
            tx,
            index,
            start,
            format!("Error: Failed to write temp file: {}", e),
            false,
        )
        .await;
        return;
    }

    let Some(resolved) = detect_tool(cmd.tool) else {
        let _ = fs::remove_file(&input_file);
        send_completed(
            tx,
            index,
            start,
            format!("Error: {} not found", cmd.tool),
            false,
        )
        .await;
        return;
    };

    let args: Vec<String> = cmd
        .args
        .iter()
        .map(|a| resolve_arg_compile(a, &temp_dir, &input_file, &output_file))
        .collect();

    let mut c = Command::new(&resolved);
    c.args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (output, success) = spawn_and_stream(&mut c, tx, index).await;
    let _ = fs::remove_file(&input_file);
    send_completed(tx, index, start, output, success).await;
}

async fn run_with_compile(
    cmd: &CommandTemplate,
    code: &str,
    tx: &mpsc::Sender<(usize, ExecutionEvent)>,
    index: usize,
    start: Instant,
) {
    let compile = cmd.compile.as_ref().expect("compile must be present");
    let Some(compile_resolved) = detect_tool(compile.tool) else {
        send_completed(
            tx,
            index,
            start,
            format!("Error: {} not found", compile.tool),
            false,
        )
        .await;
        return;
    };

    let temp_dir = std::env::temp_dir();
    let input_file = temp_dir.join("main");
    let output_file = temp_dir.join("output");

    if let Err(e) = fs::write(&input_file, code) {
        send_completed(
            tx,
            index,
            start,
            format!("Error: Failed to write temp file: {}", e),
            false,
        )
        .await;
        return;
    }

    let compile_args: Vec<String> = compile
        .args
        .iter()
        .map(|a| resolve_arg_compile(a, &temp_dir, &input_file, &output_file))
        .collect();

    let mut c = Command::new(&compile_resolved);
    c.args(&compile_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (compile_output, compile_success) = spawn_and_stream(&mut c, tx, index).await;
    let _ = fs::remove_file(&input_file);

    if !compile_success {
        let _ = fs::remove_file(&output_file);
        send_completed(
            tx,
            index,
            start,
            format!("Compile error: {}", compile_output),
            false,
        )
        .await;
        return;
    }

    let tool_to_run = resolve_arg_compile(cmd.run.tool, &temp_dir, &input_file, &output_file);

    if tool_to_run.is_empty() {
        let _ = fs::remove_file(&output_file);
        send_completed(
            tx,
            index,
            start,
            "Error: No tool to run after compilation".to_string(),
            false,
        )
        .await;
        return;
    }

    let run_args: Vec<String> = cmd
        .run
        .args
        .iter()
        .map(|a| resolve_arg_compile(a, &temp_dir, &input_file, &output_file))
        .collect();

    let mut c = Command::new(&tool_to_run);
    c.args(&run_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (output, success) = spawn_and_stream(&mut c, tx, index).await;
    let _ = fs::remove_file(&output_file);
    send_completed(tx, index, start, output, success).await;
}
