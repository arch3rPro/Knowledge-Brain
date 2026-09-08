#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]] || [[ ${3:-} != "" && ${3:-} != "--check-only" ]]; then
  echo "usage: ensure-release-tag.sh <vX.Y.Z> <commit> [--check-only]" >&2
  exit 64
fi

tag=$1
release_commit=$2
check_only=false
if [[ ${3:-} == "--check-only" ]]; then
  check_only=true
fi
if [[ ! $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "release tag must use vX.Y.Z" >&2
  exit 65
fi
release_commit=$(git rev-parse "${release_commit}^{commit}")

remote_object=""
remote_commit=""
while IFS=$'\t' read -r object name; do
  if [[ $name == "refs/tags/$tag" ]]; then
    remote_object=$object
  elif [[ $name == "refs/tags/$tag^{}" ]]; then
    remote_commit=$object
  fi
done < <(git ls-remote --tags origin "refs/tags/$tag" "refs/tags/$tag^{}")

if [[ -n $remote_object ]]; then
  if [[ -z $remote_commit ]]; then
    echo "existing release tag must be annotated" >&2
    exit 66
  fi
  if [[ $remote_commit != "$release_commit" ]]; then
    echo "existing release tag points to a different commit" >&2
    exit 67
  fi
  if git show-ref --verify --quiet "refs/tags/$tag"; then
    if [[ $(git cat-file -t "refs/tags/$tag") != tag ]] ||
      [[ $(git rev-parse "refs/tags/$tag^{}") != "$release_commit" ]] ||
      [[ $(git rev-parse "refs/tags/$tag") != "$remote_object" ]]; then
      echo "local release tag conflicts with origin" >&2
      exit 68
    fi
  else
    git fetch --quiet origin "refs/tags/$tag:refs/tags/$tag"
  fi
  echo "verified existing annotated tag $tag at $release_commit"
  exit 0
fi

if [[ $check_only == true ]]; then
  echo "release tag $tag is available"
  exit 0
fi

if git show-ref --verify --quiet "refs/tags/$tag"; then
  if [[ $(git cat-file -t "refs/tags/$tag") != tag ]] ||
    [[ $(git rev-parse "refs/tags/$tag^{}") != "$release_commit" ]]; then
    echo "local release tag conflicts with release commit" >&2
    exit 68
  fi
else
  git -c user.name='github-actions[bot]' \
    -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    tag -a "$tag" "$release_commit" -m "Knowledge-Brain $tag"
fi

git push origin "refs/tags/$tag:refs/tags/$tag"
echo "created annotated tag $tag at $release_commit"
