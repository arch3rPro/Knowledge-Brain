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
require_text .github/workflows/release.yml "workflow_dispatch:"
reject_text .github/workflows/release.yml "push:"
reject_text .github/workflows/release.yml "pull_request:"
require_text .github/workflows/release.yml 'bash scripts/check-release-candidate.sh "${GITHUB_SHA}"'
require_text .github/workflows/release.yml 'tag: ${{ steps.candidate.outputs.tag }}'
require_text .github/workflows/release.yml 'expected_version: ${{ needs.validate.outputs.version }}'
require_text .github/workflows/release.yml 'bash scripts/publish-release.sh "${{ needs.validate.outputs.tag }}" dist "${GITHUB_SHA}"'
require_text .github/workflows/native-build.yml "workflow_call:"
require_text .github/workflows/native-build.yml "fail-fast: false"
require_text .github/workflows/native-build.yml "fromJSON(inputs.targets_json)"
require_text .github/workflows/native-build.yml 'install -m 755 release-stage/kb "dist/knowledge-brain-v${{ inputs.expected_version }}-${{ matrix.target }}"'
require_text .github/workflows/native-build.yml 'Copy-Item release-stage/kb.exe "dist/knowledge-brain-v${{ inputs.expected_version }}-${{ matrix.target }}.exe"'
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
require_text scripts/publish-release.sh 'bash scripts/ensure-release-tag.sh "$tag" "$release_commit"'
reject_text scripts/publish-release.sh "--generate-notes"

asset_line=$(grep -n 'if \[\[ ${#assets\[@\]} -ne 8 \]\]' scripts/publish-release.sh | cut -d: -f1)
tag_line=$(grep -n 'bash scripts/ensure-release-tag.sh' scripts/publish-release.sh | cut -d: -f1)
[[ -n $asset_line && -n $tag_line && $asset_line -lt $tag_line ]] || fail "release assets must be validated before tag creation"

if grep -R -E 'uses: [^#[:space:]]+@(v[0-9]+|main|master|stable)([[:space:]]|$)' .github/workflows; then
  fail "actions must use immutable commit SHAs"
fi

echo "workflow contracts passed"
