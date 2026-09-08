#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: check-release-tag.sh <vX.Y.Z>" >&2
  exit 64
fi

tag=$1
if [[ ! $tag =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
  echo "release tag must use vX.Y.Z" >&2
  exit 65
fi
version=${BASH_REMATCH[1]}
cargo_version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
if [[ $version != "$cargo_version" ]]; then
  echo "tag version $version does not match Cargo.toml $cargo_version" >&2
  exit 66
fi
if [[ $(git cat-file -t "refs/tags/$tag") != tag ]]; then
  echo "release tag must be annotated" >&2
  exit 67
fi
release_commit=$(git rev-list -n 1 "$tag")
git fetch origin main
if ! git merge-base --is-ancestor "$release_commit" origin/main; then
  echo "release commit must be reachable from origin/main" >&2
  exit 68
fi
if [[ -n ${GITHUB_OUTPUT:-} ]]; then
  echo "version=$version" >> "$GITHUB_OUTPUT"
fi
echo "validated release $tag at $release_commit"
