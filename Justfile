# requires: cargo install just

# List all available commands
default:
    @just --list

# Development build
build:
    cargo build

# Release build (balanced)
release:
    cargo build --release

# Release with full LTO
lto:
    cargo build --profile release-lto

# Release optimized for size
small:
    cargo build --profile release-small

# Run tests
test:
    cargo test --all-features

# Run tests with coverage
coverage:
    cargo tarpaulin --all-features --out Html --output-dir target/coverage

# Run clippy
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Clean
clean:
    cargo clean
    rm -rf target/

# Install locally
install:
    cargo install --path crates/rspk-cli

# Build for all platforms
dist:
    cross build --release --target x86_64-unknown-linux-gnu
    cross build --release --target x86_64-unknown-linux-musl
    cross build --release --target aarch64-unknown-linux-gnu
    cross build --release --target x86_64-apple-darwin
    cross build --release --target aarch64-apple-darwin
    cross build --release --target x86_64-pc-windows-gnu

# Show binary sizes
size:
    @ls -lh target/release/pk 2>/dev/null || echo "Release: not built"
    @ls -lh target/release-lto/pk 2>/dev/null || echo "Release-LTO: not built"
    @ls -lh target/release-small/pk 2>/dev/null || echo "Release-small: not built"

# Benchmark compilation
bench-compile:
    @cargo clean
    @time cargo build
    @cargo clean
    @time cargo build --release
    @cargo clean
    @time cargo build --profile release-lto

# Generate docs
docs:
    cargo doc --no-deps --open

# Audit dependencies
audit:
    cargo audit

# Update dependencies
update:
    cargo update

# Run all checks (for CI)
check: fmt-check clippy test audit
