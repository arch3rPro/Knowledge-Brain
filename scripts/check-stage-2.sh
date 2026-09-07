#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release -p kb-cli

# Each journey creates its own Vault and isolated user directories.
KB_TEST_BINARY="$repo_root/target/release/kb" cargo test -p kb-cli --test phase2_journey
