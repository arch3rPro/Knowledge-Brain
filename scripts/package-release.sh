#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: package-release.sh <stage-dir> <asset-path> <version> <target>" >&2
  exit 64
fi

stage=$1
asset=$2
version=$3
target=$4

case "$target" in
  x86_64-unknown-linux-gnu|aarch64-apple-darwin)
    binary=kb
    extension=tar.gz
    ;;
  x86_64-pc-windows-msvc)
    binary=kb.exe
    extension=zip
    ;;
  *)
    echo "unsupported release target: $target" >&2
    exit 65
    ;;
esac

expected_name="knowledge-brain-v${version}-${target}.${extension}"
if [[ $(basename "$asset") != "$expected_name" ]]; then
  echo "asset must be named $expected_name" >&2
  exit 65
fi

for required in "$binary" LICENSE INSTALL.md; do
  if [[ ! -f "$stage/$required" ]]; then
    echo "stage is missing required file: $required" >&2
    exit 66
  fi
done

mkdir -p "$(dirname "$asset")"
rm -f "$asset"
if [[ $extension == tar.gz ]]; then
  tar -C "$stage" -czf "$asset" INSTALL.md LICENSE "$binary"
else
  (
    cd "$stage"
    zip -q "$asset" "$binary" LICENSE INSTALL.md
  )
fi
