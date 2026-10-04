#!/usr/bin/env bash
# Add a Flatpak build to the self-hosted repository and publish it. See docs/updates.md.
#
# Usage: dev/flatpak-publish.sh <build repo> <branch> <pages checkout>
#   <build repo>      The OSTree repository that flatpak-builder --repo wrote.
#   <branch>          stable, beta or nightly. The build must have been made for this branch.
#   <pages checkout>  A checkout of the gh-pages branch of abbyfluoroethane/chord-nightly. The
#                     repository is in flatpak/repo. The script commits; the caller pushes.
#
# Needs flatpak, ostree and gpg (the flatpak-github-actions container has them), and
# CHORD_FLATPAK_GPG_KEY: the armored private key that signs the repository.
set -euo pipefail

build_repo=$(realpath "${1:?build repo}"); branch=${2:?branch}; pages=$(realpath "${3:?pages checkout}")
[[ $branch =~ ^(stable|beta|nightly)$ ]] || { echo "FAIL: unknown branch $branch" >&2; exit 1; }
: "${CHORD_FLATPAK_GPG_KEY:?the signing key is missing}"

APP=space.foid.chord
BASE=https://bigaouette.com/chord-nightly/flatpak
# Each branch keeps this many builds. Older ones are pruned, so the site stays small.
KEEP=7

export GNUPGHOME; GNUPGHOME=$(mktemp -d); trap 'rm -rf "$GNUPGHOME"' EXIT
printf '%s\n' "$CHORD_FLATPAK_GPG_KEY" | gpg --batch --quiet --import
key=$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr/{print $10; exit}')

# The flatpak-github-actions container may lack the ostree command. It runs as root on Fedora.
command -v ostree >/dev/null || dnf install -y -q ostree

repo="$pages/flatpak/repo"
if [[ ! -d $repo/objects ]]; then
  mkdir -p "$repo"
  ostree init --repo="$repo" --mode=archive-z2
fi
# Git keeps no empty folders, so a repository checked out from gh-pages lacks some that ostree
# needs, such as refs/remotes.
mkdir -p "$repo"/{objects,tmp,state,extensions,refs/heads,refs/remotes,refs/mirrors}

ref="app/$APP/x86_64/$branch"
ostree --repo="$build_repo" rev-parse "$ref" >/dev/null || { echo "FAIL: $build_repo has no $ref" >&2; exit 1; }
# Copy the new commit on top of the branch, signed. build-commit-from keeps the history, so
# the updater sees a newer commit on the same branch.
flatpak build-commit-from --src-repo="$build_repo" --gpg-sign="$key" --gpg-homedir="$GNUPGHOME" \
  --update-appstream --no-update-summary "$repo" "$ref"
flatpak build-update-repo --gpg-sign="$key" --gpg-homedir="$GNUPGHOME" \
  --generate-static-deltas --prune --prune-depth="$KEEP" \
  --title="Chord nightly" "$repo"

# The files that add the repository, and one per branch that installs the app.
pub=$(gpg --export "$key" | base64 -w0)
cat > "$pages/flatpak/chord-nightly.flatpakrepo" <<EOF
[Flatpak Repo]
Title=Chord nightly
Url=$BASE/repo/
Homepage=https://bigaouette.com/chord-site/
Comment=Test builds of Chord. Stable builds will be on Flathub.
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
GPGKey=$pub
EOF
cat > "$pages/flatpak/$APP-$branch.flatpakref" <<EOF
[Flatpak Ref]
Name=$APP
Branch=$branch
Title=Chord ($branch)
Url=$BASE/repo/
SuggestRemoteName=chord-nightly
Homepage=https://bigaouette.com/chord-site/
RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo
IsRuntime=false
GPGKey=$pub
EOF
cat > "$pages/flatpak/index.html" <<EOF
<!doctype html>
<meta charset="utf-8">
<title>Chord Flatpak repository</title>
<h1>Chord Flatpak repository</h1>
<p>Install a branch, then Chord updates itself:</p>
<pre>flatpak install --from $BASE/$APP-nightly.flatpakref</pre>
<p>Branches: $(ls "$pages/flatpak" | sed -n "s/^$APP-\(.*\)\.flatpakref$/<a href=\"$APP-\1.flatpakref\">\1<\/a>/p" | paste -sd ' ')</p>
<p>The repository: <a href="chord-nightly.flatpakrepo">chord-nightly.flatpakrepo</a></p>
EOF
touch "$pages/.nojekyll"
echo "Published $ref to $BASE/repo/"
