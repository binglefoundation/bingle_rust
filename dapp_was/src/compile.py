#!/usr/bin/env python3
import os
import sys
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
DAPP_DIR = ROOT / "dapp_was"
SRC = DAPP_DIR / "src" / "app.py"
OUT = DAPP_DIR / "build"


def main() -> int:
    if not SRC.exists():
        print(f"error: source contract not found at {SRC}", file=sys.stderr)
        return 1
    OUT.mkdir(parents=True, exist_ok=True)

    # Attempt to run: python -m algopy compile dapp_was/src/app.py --out dapp_was/build
    cmd = [sys.executable, "-m", "algopy", "compile", str(SRC), "--out", str(OUT)]
    try:
        print(f"[algopy] compiling {SRC} -> {OUT} ...")
        res = subprocess.run(cmd, capture_output=True, text=True)
    except FileNotFoundError:
        print("error: failed to invoke Python; ensure Python is installed", file=sys.stderr)
        return 1

    if res.returncode != 0:
        print("error: algopy compile failed", file=sys.stderr)
        if res.stdout:
            print(res.stdout)
        if res.stderr:
            print(res.stderr, file=sys.stderr)
        print("hint: install Algorand Python tooling: pip install algorand-python", file=sys.stderr)
        print("docs: https://dev.algorand.co/concepts/smart-contracts/languages/python/", file=sys.stderr)
        return res.returncode

    # Show resulting files if present
    produced = []
    for name in ("approval.teal", "clear.teal", "contract.json"):
        p = OUT / name
        if p.exists():
            produced.append(p)
    if produced:
        print("[algopy] produced:")
        for p in produced:
            print(f"  - {p}")
    else:
        print("warning: no known output files found; check algopy version and CLI output")
        if res.stdout:
            print(res.stdout)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
