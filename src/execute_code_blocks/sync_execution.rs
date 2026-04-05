use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::execute_code_blocks::default_language_config::find_language;
use crate::execute_code_blocks::language_config::{
    CommandTemplate, ExecutableCodeBlock, LanguageConfig,
};

pub fn detect_tool(tool: &str) -> Option<PathBuf> {
    if Command::new(tool).arg("--version").output().is_ok() {
        Some(PathBuf::from(tool))
    } else {
        None
    }
}

pub fn execute_code(lang: &str, code: &str) -> Option<String> {
    let config = find_language(lang)?;
    Some(execute_language(config, code))
}

pub fn execute_code_blocks(blocks: &[ExecutableCodeBlock]) -> Vec<String> {
    blocks
        .iter()
        .map(|block| {
            execute_code(&block.lang, &block.code)
                .unwrap_or_else(|| "Error: No runtime found".to_string())
        })
        .collect()
}

fn unique_temp_dir() -> PathBuf {
    let pid = std::process::id();
    std::env::temp_dir().join(format!("literate_{}", pid))
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

pub fn resolve_arg_compile(
    arg: &str,
    temp_dir: &Path,
    input_file: &Path,
    output_file: &Path,
) -> String {
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

fn run_with_compile(
    compile_tool: &str,
    compile_args: &[&str],
    run_tool: &str,
    run_args: &[&str],
    code: &str,
    temp_dir: &Path,
) -> Option<String> {
    fs::create_dir_all(temp_dir).ok()?;

    let input_file = temp_dir.join("main");
    fs::write(&input_file, code).ok()?;

    let output_file = temp_dir.join("output");

    let resolved_compile_args: Vec<String> = compile_args
        .iter()
        .map(|a| resolve_arg_compile(a, temp_dir, &input_file, &output_file))
        .collect();

    let compile_output = Command::new(compile_tool)
        .args(&resolved_compile_args)
        .output();

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

    let tool_to_run = resolve_arg_compile(run_tool, temp_dir, &input_file, &output_file);

    let resolved_run_args: Vec<String> = run_args
        .iter()
        .map(|a| resolve_arg_compile(a, temp_dir, &input_file, &output_file))
        .collect();

    let run_output = Command::new(&tool_to_run).args(&resolved_run_args).output();

    let _ = fs::remove_file(&input_file);
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
        let tool_to_check = cmd.compile.as_ref().map(|c| c.tool).unwrap_or(cmd.run.tool);

        let Some(resolved) = detect_tool(tool_to_check) else {
            continue;
        };

        let result = if let Some(compile) = &cmd.compile {
            let temp_dir = unique_temp_dir();
            run_with_compile(
                &compile.tool,
                compile.args,
                cmd.run.tool,
                cmd.run.args,
                code,
                &temp_dir,
            )
        } else if cmd.run.inline {
            run_inline(&resolved, cmd.run.args, code)
        } else {
            let temp_dir = unique_temp_dir();
            run_with_file(&resolved, cmd.run.args, code, &temp_dir)
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
            .map(|c| c.run.tool)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_language_shell() {
        let lang = find_language("sh");
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().commands[0].run.tool, "/bin/sh");
    }

    #[test]
    fn test_find_language_python() {
        let lang = find_language("python");
        assert!(lang.is_some());
        assert_eq!(lang.unwrap().commands[0].run.tool, "python3");
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
    }

    #[test]
    fn test_find_language_unknown() {
        let lang = find_language("unknownlang");
        assert!(lang.is_none());
    }

    #[test]
    fn test_shell_execution() {
        let result = execute_code("sh", "echo hello");
        assert!(result.is_some());
        assert!(result.unwrap().contains("hello"));
    }

    #[test]
    fn test_python_execution() {
        let result = execute_code("python", "print(1 + 1)");
        assert!(result.is_some());
        assert!(result.unwrap().contains("2"));
    }

    #[test]
    fn test_node_execution() {
        let result = execute_code("js", "console.log(1 + 1)");
        assert!(result.is_some());
    }

    #[test]
    fn test_typescript_execution() {
        let result = execute_code("ts", "console.log(1 + 1)");
        assert!(result.is_some());
    }

    #[test]
    fn test_rust_execution() {
        let result = execute_code("rust", "fn main() { println!(\"hello\"); }");
        assert!(result.is_some());
    }

    #[test]
    fn test_unknown_language_skipped() {
        let lang = find_language("mermaid");
        assert!(lang.is_none());
    }

    #[test]
    fn test_detect_tool_global() {
        let result = detect_tool("ls");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_tool_missing() {
        let result = detect_tool("nonexistent_tool_12345");
        assert!(result.is_none());
    }
}
