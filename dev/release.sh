#!/usr/bin/env bash
# Release one platform. See docs/releasing.md.
#
# Usage: dev/release.sh <android|desktop> <version> [--notes <file>] [--prebuilt <dir>] [--dry-run]
#   <version>          0.3.0 for a release, 0.3.0-beta.2 for a beta.
#   --notes <file>     Release notes in Markdown. Default: the commits since the last release.
#   --prebuilt <dir>   Android: use a prebuilt core and skip cargo (see chord-android/README.md).
#   --dry-run          Build and print what would be published, but tag and publish nothing.
#
# The script tags the commit as <platform>-v<version> in this repository. For Android it builds
# the signed APKs here and publishes them to the release repository. For the desktop the tag
# starts .github/workflows/release.yml, which builds on Linux, Windows and macOS and publishes.
#
# The release repositories hold only releases:
#   abbyfluoroethane/chord-android, abbyfluoroethane/chord-desktop, abbyfluoroethane/chord-iOS.
# Their tags are v<version> for a release and v<version>+<commit> for a beta.
set -euo pipefail
cd "$(dirname "$0")/.."

OWNER=abbyfluoroethane
usage() { echo "usage: $0 <android|desktop> <version> [--notes <file>] [--prebuilt <dir>] [--dry-run]" >&2; exit 2; }

platform=${1:-}; version=${2:-}
[[ $platform == android || $platform == desktop ]] || usage
shift 2
notes_file=""; prebuilt=(); dry=0
while (( $# )); do
  case "$1" in
    --notes) notes_file=$(realpath "${2:?--notes needs a file}"); shift 2 ;;
    --prebuilt) prebuilt=(--prebuilt "$(realpath "${2:?--prebuilt needs a directory}")"); shift 2 ;;
    --dry-run) dry=1; shift ;;
    *) usage ;;
  esac
done

# gh and the git credentials live in the chord-android toolbox.
if [[ ! -f /run/.containerenv ]] || ! grep -q 'name="chord-android"' /run/.containerenv; then
  args=("$platform" "$version")
  [[ -n $notes_file ]] && args+=(--notes "$notes_file")
  args+=("${prebuilt[@]}")
  (( dry )) && args+=(--dry-run)
  exec toolbox run -c chord-android bash "$PWD/dev/release.sh" "${args[@]}"
fi

if [[ ! $version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-beta\.([1-9][0-9]*))?$ ]]; then
  echo "FAIL: $version is not MAJOR.MINOR.PATCH or MAJOR.MINOR.PATCH-beta.N" >&2
  exit 1
fi
beta=0; [[ $version == *-beta.* ]] && beta=1

if [[ -n $(git status --porcelain --untracked-files=no) ]]; then
  echo "FAIL: the working tree has changes. Commit them first, so the tag names the code that you build." >&2
  exit 1
fi
tag="$platform-v$version"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null || git ls-remote --exit-code --tags origin "$tag" >/dev/null 2>&1; then
  echo "FAIL: the tag $tag exists already." >&2
  exit 1
fi
if ! git merge-base --is-ancestor HEAD "@{upstream}" 2>/dev/null; then
  echo "FAIL: push the branch first, so the tag names a commit that is on GitHub." >&2
  exit 1
fi

commit=$(git rev-parse --short HEAD)
release_tag="v$version"; (( beta )) && release_tag="v$version+$commit"
repo="$OWNER/chord-$platform"

# Notes: the given file, or the commits for this platform since its last release.
notes=$(mktemp); trap 'rm -f "$notes"' EXIT
if [[ -n $notes_file ]]; then
  cat "$notes_file" > "$notes"
else
  case $platform in
    android) paths=(chord-android chord-core chord-ffi Cargo.lock) ;;
    desktop) paths=(chord-desktop chord-core Cargo.lock) ;;
  esac
  prev=$(git describe --tags --abbrev=0 --match "$platform-v*" 2>/dev/null || true)
  {
    if [[ -n $prev ]]; then
      echo "Changes since ${prev#"$platform-v"}:"
      echo
      git log --no-merges --format='- %s' "$prev..HEAD" -- "${paths[@]}"
    else
      echo "The first test release."
    fi
  } > "$notes"
fi
printf '\nBuilt from commit `%s` of the Chord source.\n' "$commit" >> "$notes"

if [[ $platform == desktop ]]; then
  echo "Tag $tag at $commit. The release workflow builds and publishes $repo $release_tag."
  (( dry )) && { echo "Dry run: no tag."; cat "$notes"; exit 0; }
  git tag -a "$tag" -m "Chord Desktop $version" -m "$(cat "$notes")"
  git push origin "refs/tags/$tag"
  echo "Follow it with: gh run list --workflow release.yml"
  exit 0
fi

# Android: build here, then publish.
dev/android-release.sh --version "$version" "${prebuilt[@]}"
out=chord-android/app/build/outputs/apk/release
dist=$(mktemp -d); trap 'rm -f "$notes"; rm -rf "$dist"' EXIT
for abi in arm64-v8a x86_64 universal; do
  src="$out/app-$abi-release.apk"
  [[ -f $src ]] || { echo "FAIL: no $src" >&2; exit 1; }
  cp "$src" "$dist/chord-android-$version-$abi.apk"
done
(cd "$dist" && sha256sum ./*.apk | sed 's# \./# #' > SHA256SUMS)
{
  echo
  echo "Most phones need the arm64-v8a APK. A newer build installs over an older one."
  echo
  echo '```'
  cat "$dist/SHA256SUMS"
  echo '```'
} >> "$notes"

echo "Publish $repo $release_tag ($( (( beta )) && echo beta || echo release)) and tag $tag at $commit:"
ls -l "$dist"
if (( dry )); then
  echo "Dry run: nothing published."
  cat "$notes"
  exit 0
fi

git tag -a "$tag" -m "Chord for Android $version" -m "$(cat "$notes")"
git push origin "refs/tags/$tag"
flags=(--title "Chord for Android $version" --notes-file "$notes")
if (( beta )); then flags+=(--prerelease); else flags+=(--latest); fi
gh release create "$release_tag" -R "$repo" "${flags[@]}" "$dist"/*
echo "Published: https://github.com/$repo/releases/tag/$(printf '%s' "$release_tag" | sed 's/+/%2B/g')"
