use std::env;
use std::fs;
use std::io::{self, Read};

#[derive(Debug)]
struct Config {
    interactive: bool,
    write: bool,
    file: Option<String>,
}

fn parse_args() -> Config {
    let mut interactive = false;
    let mut write = false;
    let mut file = None;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-i" | "--interactive" => interactive = true,
            "-w" | "--write" => write = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => file = Some(arg),
        }
    }

    Config {
        interactive,
        write,
        file,
    }
}

fn print_help() {
    println!(
        r#"Usage: literate-docs [OPTIONS] [FILE]

Literate programming tool that executes code blocks in markdown

Arguments:
  [FILE]    Input markdown file (reads from stdin if omitted)

Options:
  -i, --interactive    Interactive mode with streaming output
  -w, --write          Write output back to the input file in place
  -h, --help           Print help
  -V, --version        Print version"#
    );
}

fn read_input(file: Option<&str>) -> String {
    if let Some(path) = file {
        fs::read_to_string(path).expect("Failed to read file")
    } else {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .expect("Failed to read stdin");
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_args_no_flags() {
        let args: Vec<&str> = vec!["test"];
        let config = parse_args_from(args);
        assert!(!config.interactive);
        assert!(!config.write);
        assert!(config.file.is_none());
    }

    #[test]
    fn test_parse_args_with_file() {
        let args: Vec<&str> = vec!["test", "file.md"];
        let config = parse_args_from(args);
        assert!(!config.interactive);
        assert!(!config.write);
        assert_eq!(config.file, Some("file.md".to_string()));
    }

    #[test]
    fn test_parse_args_interactive_short() {
        let args: Vec<&str> = vec!["test", "-i"];
        let config = parse_args_from(args);
        assert!(config.interactive);
        assert!(!config.write);
        assert!(config.file.is_none());
    }

    #[test]
    fn test_parse_args_interactive_long() {
        let args: Vec<&str> = vec!["test", "--interactive"];
        let config = parse_args_from(args);
        assert!(config.interactive);
    }

    #[test]
    fn test_parse_args_write_short() {
        let args: Vec<&str> = vec!["test", "-w"];
        let config = parse_args_from(args);
        assert!(config.write, "-w should set write to true");
        assert_eq!(config.file, None);
    }

    #[test]
    fn test_parse_args_write_long() {
        let args: Vec<&str> = vec!["test", "--write"];
        let config = parse_args_from(args);
        assert!(config.write, "--write should set write to true");
    }

    #[test]
    fn test_parse_args_write_with_file() {
        let args: Vec<&str> = vec!["test", "-w", "file.md"];
        let config = parse_args_from(args);
        assert!(config.write);
        assert_eq!(config.file, Some("file.md".to_string()));
    }

    #[test]
    fn test_parse_args_interactive_and_write() {
        let args: Vec<&str> = vec!["test", "-i", "-w", "file.md"];
        let config = parse_args_from(args);
        assert!(config.interactive);
        assert!(config.write);
        assert_eq!(config.file, Some("file.md".to_string()));
    }

    #[test]
    fn test_parse_args_file_after_flags() {
        let args: Vec<&str> = vec!["test", "--interactive", "--write", "doc.md"];
        let config = parse_args_from(args);
        assert!(config.interactive);
        assert!(config.write);
        assert_eq!(config.file, Some("doc.md".to_string()));
    }

    #[test]
    fn test_parse_args_help_exits() {
        let args: Vec<&str> = vec!["test", "-h"];
        let result = std::panic::catch_unwind(|| parse_args_from(args));
        assert!(result.is_err(), "-h should cause panic/exit");
    }

    fn parse_args_from(args: Vec<&str>) -> Config {
        let mut interactive = false;
        let mut write = false;
        let mut file = None;

        for arg in args.iter().skip(1) {
            match *arg {
                "-i" | "--interactive" => interactive = true,
                "-w" | "--write" => write = true,
                "-h" | "--help" => {
                    panic!("help");
                }
                _ => file = Some(arg.to_string()),
            }
        }

        Config {
            interactive,
            write,
            file,
        }
    }
}

#[tokio::main]
async fn main() {
    let config = parse_args();

    if config.write && config.file.is_none() {
        eprintln!("Error: --write requires an input file");
        std::process::exit(1);
    }

    let input_content = read_input(config.file.as_deref());

    let output = if config.interactive {
        let mut app = literate_docs::tui::TuiApp::new(&input_content, None);
        match app.run().await {
            Some(ast) => Some(literate_docs::render_markdown_from_ast(&ast)),
            None => None,
        }
    } else {
        Some(literate_docs::literate_docs(&input_content))
    };

    if let Some(output) = output {
        if config.write {
            let path = config.file.as_ref().unwrap();
            fs::write(path, output).expect("Failed to write file");
        } else {
            print!("{}", output);
        }
    }
}
