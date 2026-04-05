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
    use std::fs;

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
    use std::fs;

    let tsc_path = match super::execute_code_blocks::detect_tool("tsc") {
        Some(p) => p.to_string_lossy().to_string(),
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
