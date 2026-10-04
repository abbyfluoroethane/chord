#!/usr/bin/env python3
"""Release helpers: build numbers, update manifests, channel publishing. See docs/updates.md.

Usage:
  dev/releases.py build-number <version> [<commit>]
      The build number of a version made from a commit (default HEAD).
  dev/releases.py nightly-version <android|desktop> [<yyyymmdd>]
      The version of tonight's nightly: the last release tag of the app, the next patch after a
      release, and -nightly.<date>.
  dev/releases.py android-manifest <version> <commit> <release_url> <download_base> <notes.md> <apk dir>
      Print the Android manifest. The APKs are chord-android-<version>-<abi>.apk.
  dev/releases.py desktop-manifests <version> <commit> <release_url> <download_base> <notes.md> <dist dir> <out dir>
      Write <out dir>/<target>-<arch>.json for each updater-<target>-<arch>.json in <dist dir>.
  dev/releases.py publish <android|desktop> <channel> <manifest file or dir>
      Write the manifests to the release repository: to <channel>, and to every faster channel
      whose build is older (the follow rule).
  dev/releases.py prune <repo> <prefix> <keep>
      Delete all but the newest <keep> releases (and their tags) whose tag starts with <prefix>.

GitHub calls go through the gh CLI: gh's login locally, GH_TOKEN in CI.
"""

import base64
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import urllib.parse

OWNER = "abbyfluoroethane"
EPOCH = 1_767_225_600  # 2026-01-01T00:00:00Z
CHANNELS = ["stable", "beta", "nightly"]
VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-(beta\.[1-9]\d*|nightly\.\d{8}))?$")


def die(msg):
    print(f"FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


def git(*args):
    return subprocess.check_output(["git", *args], text=True, stderr=subprocess.DEVNULL).strip()


def channel_of(version):
    m = VERSION.match(version) or die(f"{version} is not MAJOR.MINOR.PATCH, -beta.N or -nightly.YYYYMMDD")
    pre = m.group(4) or ""
    return "beta" if pre.startswith("beta") else "nightly" if pre.startswith("nightly") else "stable"


def build_number(version, commit="HEAD"):
    """Same rule as chord-android/app/build.gradle.kts and chord-desktop/src-tauri/build.rs."""
    minutes = max(0, (int(git("show", "-s", "--format=%ct", commit)) - EPOCH) // 60)
    return minutes * 4 + {"stable": 2, "beta": 1}.get(channel_of(version), 0)


def nightly_version(platform, date=None):
    date = date or datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d")
    try:
        tag = git("describe", "--tags", "--abbrev=0", "--match", f"{platform}-v*")
    except subprocess.CalledProcessError:
        tag = f"{platform}-v0.0.0"
    m = VERSION.match(tag.removeprefix(f"{platform}-v")) or die(f"bad tag {tag}")
    major, minor, patch = int(m.group(1)), int(m.group(2)), int(m.group(3))
    if not m.group(4):
        patch += 1  # after 0.3.0, nightlies lead to 0.3.1
    return f"{major}.{minor}.{patch}-nightly.{date}"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def base_fields(version, commit, release_url, notes_file):
    with open(notes_file, encoding="utf-8") as f:
        notes = f.read().strip()
    return {
        "build": build_number(version, commit),
        "channel": channel_of(version),
        "commit": git("rev-parse", "--short", commit),
        "pub_date": now(),
        "release_url": release_url,
        "notes": notes,
    }


def android_manifest(version, commit, release_url, download_base, notes_file, apk_dir):
    apks = {}
    for abi in ["arm64-v8a", "x86_64", "universal"]:
        name = f"chord-android-{version}-{abi}.apk"
        path = os.path.join(apk_dir, name)
        if not os.path.isfile(path):
            die(f"no {path}")
        apks[abi] = {"url": f"{download_base}/{name}", "sha256": sha256(path), "size": os.path.getsize(path)}
    return {"version": version, **base_fields(version, commit, release_url, notes_file), "apks": apks}


def desktop_manifests(version, commit, release_url, download_base, notes_file, dist, out):
    """Each build job leaves updater-<target>-<arch>.json: {"file": <updater bundle>, "signature": <.sig text>}."""
    os.makedirs(out, exist_ok=True)
    base = base_fields(version, commit, release_url, notes_file)
    found = 0
    for name in sorted(os.listdir(dist)):
        m = re.fullmatch(r"updater-(linux|windows|darwin)-(x86_64|aarch64)\.json", name)
        if not m:
            continue
        with open(os.path.join(dist, name), encoding="utf-8") as f:
            part = json.load(f)
        manifest = {
            "version": f"{version}+b{base['build']}",
            **base,
            "url": f"{download_base}/{urllib.parse.quote(part['file'])}",
            "signature": part["signature"],
        }
        with open(os.path.join(out, f"{m.group(1)}-{m.group(2)}.json"), "w", encoding="utf-8") as f:
            json.dump(manifest, f, indent=2)
            f.write("\n")
        found += 1
    if not found:
        die(f"no updater-*.json in {dist}")


def gh_json(*args, ok404=False):
    p = subprocess.run(["gh", "api", *args], capture_output=True, text=True)
    if p.returncode != 0:
        if ok404 and ("Not Found" in p.stdout or "404" in p.stderr):
            return None
        die(f"gh api {' '.join(args[:3])}: {p.stderr.strip() or p.stdout.strip()}")
    return json.loads(p.stdout) if p.stdout.strip() else None


def put_file(repo, path, manifest):
    """Write one manifest unless the file there holds a newer or equal build."""
    current = gh_json(f"repos/{repo}/contents/{path}", ok404=True)
    if current:
        old = json.loads(base64.b64decode(current["content"]))
        if old.get("build", 0) >= manifest["build"]:
            print(f"keep {repo}/{path}: build {old.get('build')} >= {manifest['build']}")
            return
    body = json.dumps(manifest, indent=2) + "\n"
    args = [
        "-X", "PUT", f"repos/{repo}/contents/{path}",
        "-f", f"message=Update {path} to {manifest['version']}",
        "-f", f"content={base64.b64encode(body.encode()).decode()}",
    ]
    if current:
        args += ["-f", f"sha={current['sha']}"]
    gh_json(*args)
    print(f"wrote {repo}/{path}: {manifest['version']} (build {manifest['build']})")


def publish(platform, channel, source):
    if channel not in CHANNELS:
        die(f"unknown channel {channel}")
    repo = f"{OWNER}/chord-{platform}"
    targets = CHANNELS[CHANNELS.index(channel):]  # this channel and the faster ones
    if platform == "android":
        with open(source, encoding="utf-8") as f:
            manifest = json.load(f)
        for c in targets:
            put_file(repo, f"channels/{c}.json", manifest)
    else:
        for name in sorted(os.listdir(source)):
            with open(os.path.join(source, name), encoding="utf-8") as f:
                manifest = json.load(f)
            for c in targets:
                put_file(repo, f"channels/{c}/{name}", manifest)


def prune(repo, prefix, keep):
    releases = gh_json(f"repos/{repo}/releases?per_page=100") or []
    mine = sorted((r for r in releases if r["tag_name"].startswith(prefix)), key=lambda r: r["created_at"], reverse=True)
    for r in mine[int(keep):]:
        subprocess.run(["gh", "release", "delete", r["tag_name"], "-R", repo, "--yes", "--cleanup-tag"], check=True)
        print(f"deleted {repo} {r['tag_name']}")


def main(argv):
    if len(argv) < 2:
        die(__doc__)
    cmd, args = argv[1], argv[2:]
    if cmd == "build-number" and len(args) in (1, 2):
        print(build_number(*args))
    elif cmd == "nightly-version" and len(args) in (1, 2):
        print(nightly_version(*args))
    elif cmd == "android-manifest" and len(args) == 6:
        json.dump(android_manifest(*args), sys.stdout, indent=2)
        print()
    elif cmd == "desktop-manifests" and len(args) == 7:
        desktop_manifests(*args)
    elif cmd == "publish" and len(args) == 3:
        publish(*args)
    elif cmd == "prune" and len(args) == 3:
        prune(*args)
    else:
        die(__doc__)


if __name__ == "__main__":
    main(sys.argv)
