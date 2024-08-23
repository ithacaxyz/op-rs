set positional-arguments
alias t := test
alias d := doc
alias l := lint
alias f := fmt
alias b := build

# default recipe to display help information
default:
  @just --list

# Test with all features
test:
  cargo nextest run --locked --workspace -E "kind(lib) | kind(bin) | kind(proc-macro)"

# Test docs
doc:
  cargo test --locked --workspace --doc

# Lint
lint:
  cargo clippy --workspace --examples --tests --benches --all-features \
  && cargo hack check

# Format
fmt:
  cargo +nightly fmt --all && cargo +nightly fmt --all --check

# Build
build:
  cargo build --workspace --all-features
