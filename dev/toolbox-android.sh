#!/usr/bin/env bash
# Create and provision the chord-android toolbox on Fedora. Run it on the host. You can
# run it again: each step checks first and skips what exists.
#
# The toolbox gets: Temurin JDK 21, the C toolchain, rustup with the Android targets,
# cargo-ndk, the Android SDK (platforms, build-tools, NDK, emulator, a system image),
# the AVD chord-api36, /etc/chord-android.sh, and a guard block in ~/.bashrc. The guard
# block loads the environment file in this toolbox only.
#
# The script runs itself a second time inside the toolbox (--inside) for the steps there.
set -euo pipefail

name=chord-android
avd=chord-api36
ndk_version=29.0.14206865
image="system-images;android-36;google_apis;x86_64"
sdk_packages=(
  "platform-tools"
  "emulator"
  "platforms;android-36"
  "platforms;android-37.0"
  "build-tools;36.1.0"
  "ndk;$ndk_version"
  "$image"
)
# Pin of the Google command-line tools download. Change it to move to a new version.
cmdline_tools_zip=commandlinetools-linux-11076708_latest.zip

if [[ "${1:-}" != "--inside" ]]; then
  if ! command -v toolbox >/dev/null; then
    echo "FAIL: toolbox is not installed. Run this script on a Fedora host." >&2
    exit 1
  fi
  if ! toolbox list --containers | awk 'NR > 1 { print $2 }' | grep -qx "$name"; then
    echo "Creating the toolbox $name"
    toolbox create --assumeyes "$name"
  fi
  exec toolbox run -c "$name" bash "$(readlink -f "$0")" --inside
fi

export ANDROID_HOME="${ANDROID_HOME:-$HOME/android-sdk}"

echo "1. dnf packages"
# Fedora 44 has no java-21 package. Temurin comes from the Adoptium repository.
if [[ ! -f /etc/yum.repos.d/adoptium.repo ]]; then
  sudo tee /etc/yum.repos.d/adoptium.repo >/dev/null <<'REPO'
[Adoptium]
name=Adoptium
baseurl=https://packages.adoptium.net/artifactory/rpm/fedora/$releasever/$basearch
enabled=1
gpgcheck=1
gpgkey=https://packages.adoptium.net/artifactory/api/gpg/key/public
REPO
fi
# The libraries after ncurses-term are runtime needs of the Android emulator.
sudo dnf install -y --setopt=install_weak_deps=False \
  temurin-21-jdk gcc gcc-c++ clang cmake ninja-build unzip git curl ncurses-term \
  alsa-lib nss libpulse libXcomposite libXcursor libXdamage libXi libXtst libxkbfile \
  libglvnd-gles mesa-dri-drivers mesa-vulkan-drivers

echo "2. rustup, targets, cargo-ndk"
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
rustup target add aarch64-linux-android x86_64-linux-android
if ! command -v cargo-ndk >/dev/null; then
  cargo install cargo-ndk --locked
fi

echo "3. Android command-line tools"
if [[ ! -x "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" ]]; then
  work=$(mktemp -d)
  curl -fsSL -o "$work/tools.zip" "https://dl.google.com/android/repository/$cmdline_tools_zip"
  unzip -q "$work/tools.zip" -d "$work"
  mkdir -p "$ANDROID_HOME/cmdline-tools"
  rm -rf "$ANDROID_HOME/cmdline-tools/latest"
  mv "$work/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
  rm -rf "$work"
fi
sdkmanager="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"

echo "4. SDK packages"
export JAVA_HOME=/usr/lib/jvm/java-21-temurin-jdk
yes | "$sdkmanager" --licenses >/dev/null || true
"$sdkmanager" "${sdk_packages[@]}"

echo "5. AVD $avd"
if [[ ! -d "$HOME/.android/avd/$avd.avd" ]]; then
  echo no | "$ANDROID_HOME/cmdline-tools/latest/bin/avdmanager" create avd \
    --name "$avd" --package "$image" --device pixel_7
fi

echo "6. /etc/chord-android.sh"
sudo tee /etc/chord-android.sh >/dev/null <<'ENVFILE'
# Chord Android dev environment. Only the chord-android toolbox loads this file.
export JAVA_HOME=/usr/lib/jvm/java-21-temurin-jdk
export ANDROID_HOME="$HOME/android-sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/29.0.14206865"
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
ENVFILE

echo "7. ~/.bashrc guard block"
if ! grep -qF '/etc/chord-android.sh' "$HOME/.bashrc" 2>/dev/null; then
  cat >> "$HOME/.bashrc" <<'BLOCK'

# Chord Android dev (chord-android toolbox). Runs after SDKMAN, so JDK 21 wins there.
if [ -f /run/.containerenv ] && grep -q 'name="chord-android"' /run/.containerenv; then
  . /etc/chord-android.sh
fi
BLOCK
fi

echo "OK: the $name toolbox is ready. Start the emulator with dev/android-emulator.sh."
