#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: publish-release.sh <tag> <dist-dir>" >&2
  exit 64
fi
tag=$1
dist=$2
notes_file="docs/releases/${tag}.md"
if [[ -z ${KB_UPDATE_SIGNING_KEY:-} ]]; then
  echo "KB_UPDATE_SIGNING_KEY is required" >&2
  exit 65
fi
if [[ -z ${KB_UPDATE_SIGNING_PASSWORD:-} ]]; then
  echo "KB_UPDATE_SIGNING_PASSWORD is required" >&2
  exit 65
fi
if [[ ! -f $notes_file ]]; then
  echo "release notes are required at $notes_file" >&2
  exit 68
fi

bash scripts/create-release-checksums.sh "$dist"
umask 077
secret_file=$(mktemp)
trap 'rm -f "$secret_file"' EXIT
printf '%s\n' "$KB_UPDATE_SIGNING_KEY" > "$secret_file"
printf '%s\n' "$KB_UPDATE_SIGNING_PASSWORD" | minisign -S -s "$secret_file" -m "$dist/SHA256SUMS" -x "$dist/SHA256SUMS.minisig"

release_json=$(gh release view "$tag" --json isDraft 2>/dev/null || true)
if [[ -n $release_json ]]; then
  if [[ $release_json != *'"isDraft":true'* ]]; then
    echo "refusing to modify an existing public release" >&2
    exit 66
  fi
else
  gh release create "$tag" --draft --verify-tag --title "$tag" --notes-file "$notes_file"
fi
gh release edit "$tag" --notes-file "$notes_file"

mapfile -t assets < <(find "$dist" -maxdepth 1 -type f \( -name 'knowledge-brain-*.tar.gz' -o -name 'knowledge-brain-*.zip' -o -name 'SHA256SUMS' -o -name 'SHA256SUMS.minisig' \) -print | sort)
if [[ ${#assets[@]} -ne 5 ]]; then
  echo "expected three archives, SHA256SUMS, and SHA256SUMS.minisig" >&2
  exit 67
fi
gh release upload "$tag" "${assets[@]}" --clobber
gh release edit "$tag" --draft=false --latest
