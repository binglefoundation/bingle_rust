#!/usr/bin/env python3
"""Generate spec/dependencies.md documenting direct and transitive crate
dependencies for the four workspace crates, mapping each transitive
dependency back to the direct (top-level) dependencies that pull it in."""

import json
import re
import subprocess
from collections import defaultdict

CRATES = ["rust_comms", "bingle_local", "bingle_jsi", "bingle_webserver"]
WORKSPACE = {"rust_comms", "bingle_local", "bingle_jsi", "bingle_webserver",
             "bingle_test"}

WORKSPACE_DESCRIPTIONS = {
    "rust_comms": "Core Bingle comms library: P2P messaging engine (DTLS, STUN, Algorand integration).",
    "bingle_local": "Local API crate for storing messages and contacts for Bingle.",
    "bingle_jsi": "React Native JSI bridge for Bingle using uniffi proc macros.",
    "bingle_webserver": "Axum-based web server exposing the Bingle engine over HTTP/WebSocket.",
}

LINE_RE = re.compile(r"^(\d+)(\S+) v(\S+)(.*)$")


def load_metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True, text=True, check=True,
    ).stdout
    meta = json.loads(out)
    desc = {}
    deps = {}
    for pkg in meta["packages"]:
        d = pkg.get("description") or ""
        desc[pkg["name"]] = " ".join(d.split()).rstrip(".")
        deps[pkg["name"]] = pkg["dependencies"]
    for name, d in WORKSPACE_DESCRIPTIONS.items():
        desc[name] = d.rstrip(".")
    return desc, deps


def tree(crate):
    out = subprocess.run(
        ["cargo", "tree", "-p", crate, "-e", "normal", "--no-dedupe",
         "--prefix", "depth"],
        capture_output=True, text=True, check=True,
    ).stdout
    nodes = []
    for line in out.splitlines():
        m = LINE_RE.match(line)
        if m:
            depth, name, version, _ = m.groups()
            nodes.append((int(depth), name, version))
    return nodes


def analyse(crate):
    direct = {}              # name -> version
    via = defaultdict(set)   # name -> set of top-level dep names
    versions = defaultdict(set)
    current_top = None
    for depth, name, version in tree(crate):
        if depth == 0:
            continue
        if depth == 1:
            current_top = name
            direct[name] = version
        else:
            via[name].add(current_top)
            versions[name].add(version)
    transitive = {
        name: (sorted(versions[name]), sorted(via[name]))
        for name in via if name not in direct
    }
    return direct, transitive


def direct_table(direct, desc):
    rows = ["| Crate | Version | What it does |", "|---|---|---|"]
    for name in sorted(direct):
        rows.append(f"| `{name}` | {direct[name]} | {desc.get(name, '')} |")
    return rows


def transitive_table(items, desc):
    rows = ["| Crate | Version(s) | Via | What it does |", "|---|---|---|---|"]
    for name, (vers, tops) in sorted(items.items()):
        via_s = ", ".join(f"`{t}`" for t in tops)
        rows.append(
            f"| `{name}` | {', '.join(vers)} | {via_s} | {desc.get(name, '')} |")
    return rows


def main():
    desc, meta_deps = load_metadata()
    lines = [
        "# Workspace crate dependencies",
        "",
        "Regenerate with `python3 scripts/gen_dependencies_doc.py` from the repo root.",
        "",
        "Generated with `cargo tree -e normal` (runtime dependencies only;",
        "direct dev- and build-dependencies are listed at the end). For each",
        "transitive dependency, the **Via** column lists the direct dependencies",
        "whose subtree pulls it in.",
        "",
        "`bingle_local`, `bingle_jsi` and `bingle_webserver` all depend on",
        "`rust_comms`, so they inherit its entire tree. To avoid repeating ~200",
        "rows per crate, transitive dependencies reached *only* through workspace",
        "crates are summarised with a count; the tables list dependencies that a",
        "non-workspace direct dependency also pulls in.",
        "",
    ]

    for crate in CRATES:
        direct, transitive = analyse(crate)
        lines += [f"## {crate}", "", desc.get(crate, "") + ".", "",
                  "### Direct dependencies", ""]
        lines += direct_table(direct, desc)
        lines.append("")

        inherited = {n: v for n, v in transitive.items()
                     if set(v[1]) <= WORKSPACE}
        own = {n: v for n, v in transitive.items() if n not in inherited}

        lines.append(f"### Transitive dependencies ({len(transitive)})")
        lines.append("")
        if inherited:
            srcs = sorted({t for v in inherited.values() for t in v[1]})
            src_s = " / ".join(f"`{s}`" for s in srcs)
            lines.append(
                f"{len(inherited)} crates are inherited solely via the "
                f"workspace dependencies {src_s} — see the `rust_comms` "
                f"section above for what each does. The "
                f"{len(own)} crates below are (also) pulled in by this "
                f"crate's own direct dependencies:")
            lines.append("")
        lines += transitive_table(own, desc)
        lines.append("")

    # dev- and build-dependencies (direct only, from cargo metadata)
    lines += ["## Direct dev- and build-dependencies", "",
              "Not part of the shipped artifacts; used for tests and builds.",
              ""]
    for crate in CRATES:
        entries = []
        for d in meta_deps.get(crate, []):
            if d["kind"] in ("dev", "build"):
                entries.append((d["kind"], d["name"], d["req"]))
        if not entries:
            continue
        lines += [f"### {crate}", "",
                  "| Crate | Kind | Version req | What it does |",
                  "|---|---|---|---|"]
        for kind, name, req in sorted(entries, key=lambda e: (e[0], e[1])):
            d = "Test helper crate for Bingle" if name == "bingle_test" \
                else desc.get(name, "")
            lines.append(f"| `{name}` | {kind} | {req} | {d} |")
        lines.append("")

    with open("spec/dependencies.md", "w") as f:
        f.write("\n".join(lines))
    print("wrote spec/dependencies.md")


if __name__ == "__main__":
    main()
