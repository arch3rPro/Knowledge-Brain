#!/usr/bin/env bash
set -euo pipefail

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
artifact_dir=${STAGE1_ARTIFACT_DIR:-"$repo_root/target/stage-1-artifacts"}
mkdir -p -- "$artifact_dir"

run_root=$(mktemp -d "${TMPDIR:-/tmp}/knowledge-brain-stage1.XXXXXX")
chmod 700 "$run_root"
cleanup() {
  if [[ -n "${run_root:-}" && "$run_root" != "/" && -d "$run_root" ]]; then
    rm -rf -- "$run_root"
  fi
}
trap cleanup EXIT

export KB_CONFIG_DIR="$run_root/user-config"
export KB_STATE_DIR="$run_root/user-state"
export KB_CACHE_DIR="$run_root/user-cache"

cd "$repo_root"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets
cargo build --release -p kb-cli

kb="$repo_root/target/release/kb"
"$kb" version --json > "$artifact_dir/version.json"
"$kb" capabilities --json > "$artifact_dir/capabilities.json"

new_vault="$run_root/new-vault"
"$kb" init "$new_vault" --json > "$artifact_dir/init.json"
mkdir "$new_vault/Notes"
"$kb" config admission add notes Notes --vault "$new_vault" --yes --json > "$artifact_dir/admission.json"
vault_id=$(sed -n 's/^vault_id: "\([^"]*\)"$/\1/p' "$new_vault/.kb/config.yml")
if [[ -z "$vault_id" ]]; then
  echo "Could not read vault_id from generated configuration." >&2
  exit 1
fi

moved_vault="$run_root/moved-vault"
mv "$new_vault" "$moved_vault"
"$kb" vault rebind "$vault_id" "$moved_vault" --json > "$artifact_dir/rebind.json"
"$kb" status --vault "$vault_id" --json > "$artifact_dir/reopened-status.json"
"$kb" doctor --vault "$vault_id" --json > "$artifact_dir/doctor.json"

existing="$run_root/existing"
mkdir -p "$existing/Notes"
printf '# Existing\n\nHuman text.\n' > "$existing/Notes/keep.md"
"$kb" adopt "$existing" --json > "$artifact_dir/adopt-plan.json"
operation_id=$(sed -n 's/.*"operation_id":"\([^"]*\)".*/\1/p' "$artifact_dir/adopt-plan.json")
if [[ -z "$operation_id" ]]; then
  echo "Could not read operation_id from adoption plan." >&2
  exit 1
fi
"$kb" apply "$operation_id" --json > "$artifact_dir/adopt-result.json"
"$kb" status --vault "$existing" --json > "$artifact_dir/adopted-status.json"

test "$(cat "$existing/Notes/keep.md")" = $'# Existing\n\nHuman text.'
test ! -e "$existing/.git"
