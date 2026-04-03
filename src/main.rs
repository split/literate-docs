use std::env;
use std::fs;
use std::io::{self, Read};

#[derive(Debug)]
struct Config {
    interactive: bool,
    file: Option<String>,
}

fn parse_args() -> Config {
    let mut interactive = false;
    let mut file = None;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-i" | "--interactive" => interactive = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            _ => file = Some(arg),
        }
    }

    Config { interactive, file }
}

fn print_help() {
    println!(
        r#"Usage: literate-docs [OPTIONS] [FILE]

Literate programming tool that executes code blocks in markdown

Arguments:
  [FILE]    Input markdown file (reads from stdin if omitted)

Options:
  -i, --interactive    Interactive mode with streaming output
  -h, --help           Print help
  -V, --version        Print version"#
    );
}

fn read_input(file: Option<&str>) -> String {
    if let Some(path) = file {
        fs::read_to_string(path).expect("Failed to read file")
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).expect("Failed to read stdin");
        buffer
    }
}

#[tokio::main]
async fn main() {
    let config = parse_args();
    let input_content = read_input(config.file.as_deref());

    if config.interactive {
        let mut app = literate_docs::tui::TuiApp::new(&input_content, None);
        app.run().await;
    } else {
        let output = literate_docs::literate_docs(&input_content);
        print!("{}", output);
    }
}
