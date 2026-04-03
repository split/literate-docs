# literate-docs

A literate programming tool that parses markdown files containing code blocks, executes the code, and embeds the output back into the document.

## Features

- **Execute code blocks** - Runs code in known languages and captures output
- **Output formats** - Supports both code block (` ```output `) and comment (`<!-- output: -->`) formats
- **Idempotent** - Running twice produces the same output (quine-like property)
- **Skip unknown languages** - Code blocks with unsupported languages are passed through unchanged
- **Format preservation** - Output format matches any existing output block
- **Interactive TUI** - Live scrollable document with streaming output

## Supported Languages

- Shell: `sh`, `bash`, `shell`
- Python: `python`, `python3`
- JavaScript: `js`, `javascript`, `node`
- Ruby, Perl, PHP, Go, Rust

## Installation

```bash
# Build from source
cargo build --release

# Or run directly
cargo run -- your-file.md
```

## Usage

```sh
literate-docs --help
```

```output
Usage: literate-docs [OPTIONS] [FILE]

Literate programming tool that executes code blocks in markdown

Arguments:
  [FILE]    Input markdown file (reads from stdin if omitted)

Options:
  -i, --interactive    Interactive mode with streaming output
  -h, --help           Print help
  -V, --version        Print version
```

## Examples

### Input

````markdown
```sh
echo "Hello, World!"
```
````

### Output

````markdown
```sh
echo "Hello, World!"
```

```output
Hello, World!
```
````

### Comment Format

If the input already contains a comment output block:

````markdown
```sh
echo "Hello"
```

<!-- output: Hello -->
````

The tool will preserve the comment format and produce:

````markdown
```sh
echo "Hello"
```

<!-- output: Hello -->
````

### Idempotency

Running the tool multiple times produces the same result:

```bash
literate-docs input.md > output.md
literate-docs output.md > output.md  # Same result
```

## How It Works

1. Parse markdown to extract code blocks
2. Identify known language code blocks
3. Execute code and capture stdout
4. Add output block after the code block
5. Use format detection to match existing output style

## License

MIT
