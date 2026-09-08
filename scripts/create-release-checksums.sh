#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: create-release-checksums.sh <dist-dir>" >&2
  exit 64
fi

dist=$1
mapfile -t archives < <(find "$dist" -maxdepth 1 -type f \( -name 'knowledge-brain-*.tar.gz' -o -name 'knowledge-brain-*.zip' \) -print | sort)
if [[ ${#archives[@]} -ne 3 ]]; then
  echo "expected exactly three release archives, found ${#archives[@]}" >&2
  exit 65
fi
: > "$dist/SHA256SUMS"
for archive in "${archives[@]}"; do
  digest=$(sha256sum "$archive" | cut -d ' ' -f 1)
  echo "$digest  $(basename "$archive")" >> "$dist/SHA256SUMS"
done
