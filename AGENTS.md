# Agent Guidelines

## Before Committing

Always run these before committing:

```bash
cargo fmt
cargo test
```

## Building and Running

Use the actual binary, not `cargo run` or `target/debug/`:

```bash
# Build
cargo build

# Run - use the binary directly
./target/debug/literate-docs [args]

# NOT: cargo run -- [args]
# NOT: target/debug/literate-docs [args]
```

## Testing

```bash
cargo test
```
