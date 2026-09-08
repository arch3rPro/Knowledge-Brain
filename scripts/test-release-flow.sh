#!/usr/bin/env bash
set -euo pipefail

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
candidate_script="$project_root/scripts/check-release-candidate.sh"
tag_script="$project_root/scripts/ensure-release-tag.sh"
checksum_script="$project_root/scripts/create-release-checksums.sh"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT

fail() {
  echo "release flow test failed: $*" >&2
  exit 1
}

expect_failure() {
  if "$@" >/dev/null 2>&1; then
    fail "command unexpectedly succeeded: $*"
  fi
}

git_quiet() {
  git "$@" >/dev/null
}

candidate_repo="$fixture_root/candidate"
mkdir -p "$candidate_repo/docs/releases"
git_quiet -C "$candidate_repo" init -b main
git_quiet -C "$candidate_repo" config user.name Test
git_quiet -C "$candidate_repo" config user.email test@example.com
printf '[workspace.package]\nversion = "1.2.3"\n' > "$candidate_repo/Cargo.toml"
printf '# Version 1.2.3\n' > "$candidate_repo/docs/releases/v1.2.3.md"
git_quiet -C "$candidate_repo" add Cargo.toml docs/releases/v1.2.3.md
git_quiet -C "$candidate_repo" commit -m candidate
candidate_commit=$(git -C "$candidate_repo" rev-parse HEAD)
output_file="$fixture_root/github-output"
(
  cd "$candidate_repo"
  GITHUB_OUTPUT="$output_file" bash "$candidate_script" "$candidate_commit" >/dev/null
)
grep -Fxq 'version=1.2.3' "$output_file" || fail "candidate version output is missing"
grep -Fxq 'tag=v1.2.3' "$output_file" || fail "candidate tag output is missing"
expect_failure bash -c "cd '$candidate_repo' && bash '$candidate_script' '${candidate_commit}^'"
sed -i.bak 's/1\.2\.3/1.2.3-beta.1/' "$candidate_repo/Cargo.toml"
expect_failure bash -c "cd '$candidate_repo' && bash '$candidate_script' '$candidate_commit'"
mv "$candidate_repo/Cargo.toml.bak" "$candidate_repo/Cargo.toml"
git_quiet -C "$candidate_repo" rm docs/releases/v1.2.3.md
git_quiet -C "$candidate_repo" commit -m remove-notes
candidate_commit=$(git -C "$candidate_repo" rev-parse HEAD)
expect_failure bash -c "cd '$candidate_repo' && bash '$candidate_script' '$candidate_commit'"

origin="$fixture_root/origin.git"
publisher="$fixture_root/publisher"
observer="$fixture_root/observer"
git_quiet init --bare "$origin"
git_quiet clone "$origin" "$publisher"
git_quiet -C "$publisher" config user.name Test
git_quiet -C "$publisher" config user.email test@example.com
printf 'release\n' > "$publisher/release.txt"
git_quiet -C "$publisher" add release.txt
git_quiet -C "$publisher" commit -m release
release_commit=$(git -C "$publisher" rev-parse HEAD)
git_quiet -C "$publisher" push -u origin HEAD:main
(
  cd "$publisher"
  bash "$tag_script" v1.2.3 "$release_commit" >/dev/null
  bash "$tag_script" v1.2.3 "$release_commit" >/dev/null
)
[[ $(git -C "$publisher" cat-file -t refs/tags/v1.2.3) == tag ]] || fail "created tag is not annotated"
[[ $(git -C "$publisher" rev-parse 'refs/tags/v1.2.3^{}') == "$release_commit" ]] || fail "created tag points to wrong commit"

printf 'next\n' >> "$publisher/release.txt"
git_quiet -C "$publisher" commit -am next
next_commit=$(git -C "$publisher" rev-parse HEAD)
expect_failure bash -c "cd '$publisher' && bash '$tag_script' v1.2.3 '$next_commit'"

git_quiet clone "$origin" "$observer"
git_quiet -C "$observer" config user.name Test
git_quiet -C "$observer" config user.email test@example.com
git_quiet -C "$observer" tag v1.2.4 "$release_commit"
git_quiet -C "$observer" push origin refs/tags/v1.2.4
expect_failure bash -c "cd '$observer' && bash '$tag_script' v1.2.4 '$release_commit'"

git_quiet -C "$observer" tag -a v2.0.0 "$release_commit" -m remote
git_quiet -C "$observer" push origin refs/tags/v2.0.0
git_quiet -C "$publisher" tag -a v2.0.0 "$next_commit" -m local
expect_failure bash -c "cd '$publisher' && bash '$tag_script' v2.0.0 '$release_commit'"

dist="$fixture_root/dist"
mkdir -p "$dist"
touch "$dist/knowledge-brain-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
touch "$dist/knowledge-brain-v1.2.3-aarch64-apple-darwin.tar.gz"
touch "$dist/knowledge-brain-v1.2.3-x86_64-pc-windows-msvc.zip"
bash "$checksum_script" "$dist"
[[ $(wc -l < "$dist/SHA256SUMS" | tr -d ' ') == 3 ]] || fail "checksum list must contain three archives"

echo "release flow tests passed"
