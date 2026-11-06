set shell := ["bash", "-euo", "pipefail", "-c"]

cargo := "cargo --quiet"
manifest := "--manifest-path tools/Cargo.toml"

default:
    @just --list

# Check apps (all, or those named): format, policy, and with the registries whether the images exist
validate *ids:
    {{ cargo }} run {{ manifest }} --bin catalog -- validate --apps apps {{ ids }}

# Check apps without asking the registries
validate-offline *ids:
    {{ cargo }} run {{ manifest }} --bin catalog -- validate --offline --apps apps {{ ids }}

# Build index.json and the icons into dist/, as CI publishes them
index:
    rm -rf dist
    {{ cargo }} run {{ manifest }} --bin catalog -- index --apps apps --out dist

# Format the validator's code
fmt:
    cargo fmt {{ manifest }} --all

# Check formatting and lints of the validator; fails on any warning
lint:
    cargo fmt {{ manifest }} --all -- --check
    cargo clippy {{ manifest }} --workspace --all-targets --locked -- -D warnings

# Test the validator (needs Docker with the Compose plugin)
test:
    cargo test {{ manifest }} --workspace --locked

# Everything CI checks, without the registries
check: lint test validate-offline
