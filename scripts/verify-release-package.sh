#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: verify-release-package.sh <asset-path> <target>" >&2
  exit 64
fi

asset=$1
target=$2

case "$target" in
  x86_64-unknown-linux-gnu|aarch64-apple-darwin)
    entries=$(tar -tzf "$asset")
    expected=$'INSTALL.md\nLICENSE\nkb'
    ;;
  x86_64-pc-windows-msvc)
    entries=$(unzip -Z1 "$asset")
    expected=$'INSTALL.md\nLICENSE\nkb.exe'
    ;;
  *)
    echo "unsupported release target: $target" >&2
    exit 65
    ;;
esac

if [[ $entries != "$expected" ]]; then
  echo "unexpected archive entries:" >&2
  printf '%s\n' "$entries" >&2
  exit 66
fi
