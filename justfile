default:
    @just --list

# Run the CLI
run *args:
    cargo run --package dedup-cli -- {{args}}

# Build the CLI in release mode
build-cli:
    cargo build --release --package dedup-cli

# Run all tests
test:
    cargo test --workspace

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# Check the workspace (fast compile check)
check:
    cargo check --workspace

# Run clippy lints
lint:
    cargo clippy --workspace -- -D warnings

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without applying
fmt-check:
    cargo fmt --all -- --check

# Install frontend dependencies
install-frontend:
    npm ci --prefix app

# Run the Tauri dev server
dev: install-frontend
    npm run tauri dev --prefix app

# Build the Tauri app in release mode
build-app: install-frontend
    npm run tauri build --prefix app

# Run all checks (CI equivalent)
ci: fmt-check lint test

# Clean build artifacts
clean:
    cargo clean
