#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "workflow contract failed: $*" >&2
  exit 1
}

require_file() {
  [[ -f $1 ]] || fail "missing $1"
}

require_text() {
  grep -Fq -- "$2" "$1" || fail "$1 must contain: $2"
}

reject_text() {
  if grep -Fq -- "$2" "$1"; then
    fail "$1 must not contain: $2"
  fi
}

[[ ! -e .github/workflows/ci.yml ]] || fail "push-triggered ci.yml must be removed"
for workflow in native-build.yml verify.yml release.yml workflow-contract.yml; do
  require_file ".github/workflows/$workflow"
done

reject_text .github/workflows/verify.yml "push:"
reject_text .github/workflows/verify.yml "pull_request:"
reject_text .github/workflows/workflow-contract.yml "push:"
reject_text .github/workflows/workflow-contract.yml "pull_request:"
require_text .github/workflows/verify.yml "workflow_dispatch:"
require_text .github/workflows/workflow-contract.yml "workflow_dispatch:"
require_text .github/workflows/release.yml "tags: ['v*']"
require_text .github/workflows/native-build.yml "workflow_call:"
require_text .github/workflows/native-build.yml "fail-fast: false"
require_text .github/workflows/native-build.yml "fromJSON(inputs.targets_json)"
reject_text .github/workflows/native-build.yml "Build native verification binary"
reject_text .github/workflows/native-build.yml "attestations: write"
reject_text .github/workflows/native-build.yml "id-token: write"
require_text .github/workflows/release.yml "KB_UPDATE_SIGNING_KEY"
require_text .github/workflows/release.yml "KB_UPDATE_SIGNING_PASSWORD"
require_text .github/workflows/release.yml "MINISIGN_VERSION: '0.12'"
require_text .github/workflows/release.yml "9a599b48ba6eb7b1e80f12f36b94ceca7c00b7a5173c95c3efc88d9822957e73"
reject_text .github/workflows/release.yml "cargo install minisign"
require_text .github/workflows/release.yml "actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093"
require_text .github/workflows/native-build.yml "actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803"
reject_text .github/workflows/native-build.yml "actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683"
require_text scripts/publish-release.sh 'notes_file="docs/releases/${tag}.md"'
require_text scripts/publish-release.sh '--notes-file "$notes_file"'
reject_text scripts/publish-release.sh "--generate-notes"

if grep -R -E 'uses: [^#[:space:]]+@(v[0-9]+|main|master|stable)([[:space:]]|$)' .github/workflows; then
  fail "actions must use immutable commit SHAs"
fi

echo "workflow contracts passed"
