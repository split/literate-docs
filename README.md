# literate-docs

A literate programming tool that parses markdown files containing code blocks, executes the code, and embeds the output back into the document.

## Features

* **Execute code blocks** - Runs code in known languages marked with `exec`
* **Hidden code blocks** - Use HTML comments (`<!-- sh exec: code -->`) to hide source in markdown renderers
* **Output formats** - Supports both code block (` ```output `) and comment (`<!-- output: -->`) formats
* **Idempotent** - Running twice produces the same output (quine-like property)
* **Skip unknown languages** - Code blocks with unsupported languages are passed through unchanged
* **Format preservation** - Output format matches any existing output block
* **Interactive TUI** - Live scrollable document with streaming output

## Supported Languages

* Shell: `sh`, `bash`, `shell`
* Python: `python`, `python3`
* JavaScript: `js`, `javascript`, `node`
* Ruby, Perl, PHP, Go, Rust

## Installation

```bash
# Build from source
cargo build --release

# Or run directly
cargo run -- your-file.md
```

## Usage

```sh exec
literate-docs --help
```

```output
Usage: literate-docs [OPTIONS] [FILE]

Literate programming tool that executes code blocks in markdown

Arguments:
  [FILE]    Input markdown file (reads from stdin if omitted)

Options:
  -i, --interactive    Interactive mode with streaming output
  -w, --write          Write output back to the input file in place
  -h, --help           Print help
  -V, --version        Print version
```

## Examples

### Input

````markdown
```sh exec
echo "Hello, World!"
```
````

### Output

````markdown
```sh exec
echo "Hello, World!"
```

```output
Hello, World!
```
````

### Comment Format

If the input already contains a comment output block:

```sh exec
echo "Hello"
```

<!-- output: Hello -->

The tool will preserve the comment format and produce:

```sh exec
echo "Hello"
```

<!-- output: Hello -->

### Hidden Code Blocks

You can hide the source code in markdown renderers using HTML comments:

```markdown
<!-- sh exec: echo "Hidden in renderers" -->
```

The source code is hidden (invisible in most markdown renderers), but the output is shown:

```markdown
<!-- sh exec: echo "Hidden in renderers" -->

```output
Hidden in renderers
```
```

<!-- output: Hello -->
````

The tool will preserve the comment format and produce:

````markdown
```sh exec
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
2. Identify code blocks with a supported language and the `exec` keyword
3. Execute code and capture stdout
4. Add output block after the code block
5. Use format detection to match existing output style

## License

MIT
