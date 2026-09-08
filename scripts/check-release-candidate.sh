#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: check-release-candidate.sh <commit>" >&2
  exit 64
fi

release_commit=$1
head_commit=$(git rev-parse HEAD)
if [[ $release_commit != "$head_commit" ]]; then
  echo "release commit must be the checked-out HEAD" >&2
  exit 65
fi

version=$(sed -n '/^\[workspace\.package\]$/,/^\[/ s/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
if [[ ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "workspace version must use stable X.Y.Z format" >&2
  exit 66
fi

tag="v${version}"
notes_file="docs/releases/${tag}.md"
if [[ ! -f $notes_file ]] || ! git ls-files --error-unmatch "$notes_file" >/dev/null 2>&1; then
  echo "committed release notes are required at $notes_file" >&2
  exit 67
fi

if [[ -n ${GITHUB_OUTPUT:-} ]]; then
  {
    echo "version=$version"
    echo "tag=$tag"
  } >> "$GITHUB_OUTPUT"
fi
echo "validated release candidate $tag at $release_commit"
