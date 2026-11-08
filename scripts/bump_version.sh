#!/usr/bin/env bash
# Set the workspace version: every crate's [package] version, the version pins on the workspace's
# path dependencies, bingle_jsi/package.json, and the lockfile (issue #399).
#
#   scripts/bump_version.sh 0.3.31        # set an exact version
#   scripts/bump_version.sh --next-patch  # 0.3.30 -> 0.3.31 (also --next-minor, --next-major)
#   scripts/bump_version.sh --current     # print the current version and exit
#   scripts/bump_version.sh --check       # fail unless every crate and the npm module agree
#
# `staging` always carries the next, unreleased version; scripts/release.sh calls this with
# --next-patch straight after a release. Run from anywhere in the repository.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

die() { printf 'error: %s\n' "$*" >&2; exit 1; }
[[ $# -eq 1 ]] || die "usage: scripts/bump_version.sh <version> | --next-patch | --next-minor | --next-major | --current | --check"

python3 - "$1" <<'PY'
import pathlib, re, sys

MEMBERS = ["bingle_core", "bingle_local", "bingle_jsi", "bingle_webserver", "bingle_test", "bingle_cli"]
PACKAGE_VERSION = re.compile(r'(\[package\][^\[]*?\nversion\s*=\s*")([^"]*)(")', re.S)
PATH_PIN = re.compile(r'(path\s*=\s*"\.\./bingle_[a-z_]+"\s*,\s*version\s*=\s*")([^"]*)(")')
NPM_VERSION = re.compile(r'("version"\s*:\s*")([^"]*)(")')
SEMVER = re.compile(r'^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.]+)?$')

def versions():
    found = {}
    for m in MEMBERS:
        text = (pathlib.Path(m) / "Cargo.toml").read_text()
        mo = PACKAGE_VERSION.search(text)
        if not mo:
            sys.exit(f"error: no [package] version in {m}/Cargo.toml")
        found[f"{m}/Cargo.toml"] = mo.group(2)
        for pin in PATH_PIN.finditer(text):
            found.setdefault(f"{m}/Cargo.toml path pins", pin.group(2))
            if pin.group(2) != mo.group(2):
                found[f"{m}/Cargo.toml path pins"] = pin.group(2)
    mo = NPM_VERSION.search(pathlib.Path("bingle_jsi/package.json").read_text())
    if not mo:
        sys.exit("error: no version in bingle_jsi/package.json")
    found["bingle_jsi/package.json"] = mo.group(2)
    return found

arg = sys.argv[1]
found = versions()
current = found["bingle_core/Cargo.toml"]
if arg == "--current":
    print(current)
    sys.exit(0)
if arg == "--check":
    odd = {k: v for k, v in found.items() if v != current}
    if odd:
        sys.exit("error: versions disagree with bingle_core " + current + ": " + ", ".join(f"{k} {v}" for k, v in odd.items()))
    print(current)
    sys.exit(0)

if arg in ("--next-patch", "--next-minor", "--next-major"):
    core = current.split("-", 1)[0].split("+", 1)[0]
    major, minor, patch = (int(x) for x in core.split(".")[:3])
    version = {"--next-patch": f"{major}.{minor}.{patch + 1}",
               "--next-minor": f"{major}.{minor + 1}.0",
               "--next-major": f"{major + 1}.0.0"}[arg]
elif SEMVER.match(arg):
    version = arg
else:
    sys.exit(f"error: not a version or option: {arg}")

for m in MEMBERS:
    p = pathlib.Path(m) / "Cargo.toml"
    text, n = PACKAGE_VERSION.subn(lambda mo: mo.group(1) + version + mo.group(3), p.read_text(), count=1)
    if n != 1:
        sys.exit(f"error: failed to set [package] version in {p}")
    p.write_text(PATH_PIN.sub(lambda mo: mo.group(1) + version + mo.group(3), text))
pj = pathlib.Path("bingle_jsi/package.json")
text, n = NPM_VERSION.subn(lambda mo: mo.group(1) + version + mo.group(3), pj.read_text(), count=1)
if n != 1:
    sys.exit("error: failed to set the version in bingle_jsi/package.json")
pj.write_text(text)
print(f"{current} -> {version}")
PY

case "$1" in
  --current|--check) exit 0 ;;
esac
# refresh the lockfile's workspace-member versions only (no dependency changes).
cargo update --workspace --quiet
