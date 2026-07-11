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
    pnpm -C app install --frozen-lockfile

# Run the Tauri dev server
dev: install-frontend
    pnpm -C app tauri dev

# Build the Tauri app in release mode
build-app: install-frontend
    pnpm -C app tauri build

# Preview the next version bump + changelog without changing anything
changelog-preview:
    knope prepare-release --dry-run

# Open/update the release PR locally (normally run by CI on push to main)
release-pr:
    knope prepare-release

# Run all checks (CI equivalent)
ci: fmt-check lint test

# Clean build artifacts
clean:
    cargo clean
