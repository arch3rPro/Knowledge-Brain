#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: create-release-checksums.sh <dist-dir>" >&2
  exit 64
fi

dist=$1
release_files=()
while IFS= read -r release_file; do
  release_files+=("$release_file")
done < <(find "$dist" -maxdepth 1 -type f \( \
  -name 'knowledge-brain-v*-x86_64-unknown-linux-gnu.tar.gz' -o \
  -name 'knowledge-brain-v*-aarch64-apple-darwin.tar.gz' -o \
  -name 'knowledge-brain-v*-x86_64-pc-windows-msvc.zip' -o \
  -name 'knowledge-brain-v*-x86_64-unknown-linux-gnu' -o \
  -name 'knowledge-brain-v*-aarch64-apple-darwin' -o \
  -name 'knowledge-brain-v*-x86_64-pc-windows-msvc.exe' \
\) -print | sort)
if [[ ${#release_files[@]} -ne 6 ]]; then
  echo "expected exactly three release archives and three executables, found ${#release_files[@]}" >&2
  exit 65
fi
: > "$dist/SHA256SUMS"
for release_file in "${release_files[@]}"; do
  digest=$(sha256sum "$release_file" | cut -d ' ' -f 1)
  echo "$digest  $(basename "$release_file")" >> "$dist/SHA256SUMS"
done
