use std::env;
use std::fs;
use std::io::{self, Read};
use literate_docs::render_markdown;

fn main() {
    let args: Vec<String> = env::args().collect();
    
    let input_content = if args.len() > 1 {
        fs::read_to_string(&args[1]).expect("Failed to read file")
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).expect("Failed to read stdin");
        buffer
    };

    let output = render_markdown(&input_content);
    
    print!("{}", output);
}