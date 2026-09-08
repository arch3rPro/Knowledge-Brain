#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT

kb_bin=${KB_BIN:-"$repo_root/target/debug/kb"}
if [[ ! -x "$kb_bin" ]]; then
  (cd "$repo_root" && cargo build -p kb-cli >/dev/null)
fi

all="$workspace/all"
one="$workspace/one"
mkdir -p "$all" "$one"

export KB_CONFIG_DIR="$workspace/config"
export KB_STATE_DIR="$workspace/state"
export KB_CACHE_DIR="$workspace/cache"
export KB_AGENT_HOME="$workspace/agent-home"
export KB_AGENT_CONFIG_DIR="$workspace/agent-config"

"$kb_bin" init "$all" --json >/dev/null
(cd "$all" && npx skills add "$repo_root" --all -a codex -y)

"$kb_bin" init "$one" --json >/dev/null
(cd "$one" && npx skills add "$repo_root" --skill kb-query -a codex -y)

test -f "$all/.agents/skills/kb-vault/SKILL.md"
test -f "$all/.agents/skills/kb-connect/SKILL.md"
test -f "$one/.agents/skills/kb-query/SKILL.md"
test ! -e "$one/.agents/skills/kb-vault/SKILL.md"
test ! -e "$all/.agents/skills/knowledge-brain"

external="$all/.agents/skills/kb-query/SKILL.md"
snapshot="$workspace/kb-query.before"
cp "$external" "$snapshot"
status=$("$kb_bin" skills status --host codex --scope vault --vault "$all" --json)
case "$status" in
  *'"state":"external"'*) ;;
  *) echo "npx-installed Skill was not reported as external: $status" >&2; exit 1 ;;
esac

if "$kb_bin" skills install --host codex --scope vault --vault "$all" --json >/dev/null 2>&1; then
  echo "kb skills install unexpectedly accepted external npx files" >&2
  exit 1
fi
if "$kb_bin" skills uninstall --host codex --scope vault --vault "$all" --json >/dev/null 2>&1; then
  echo "kb skills uninstall unexpectedly accepted external npx files" >&2
  exit 1
fi
cmp -s "$snapshot" "$external"
