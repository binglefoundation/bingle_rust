# Bingle DApp Skeleton (Algorand Python)

This directory contains a minimal Algorand DApp skeleton implemented using Algorand Python (algopy), as documented here:
https://dev.algorand.co/concepts/smart-contracts/languages/python/

Contents:
- src/app.py: Algorand Python ARC-4 contract with a simple `fn(uint64)uint64` ABI method.
- src/compile.py: Helper that invokes the `algopy` CLI to compile the contract to TEAL into `dapp/build`.
- requirements.txt: Python dependencies (Algorand Python).
- .gitignore: Local ignores for Python artifacts and build output.

Prerequisites:
- Python 3.10+
- A virtual environment (recommended)
- Algorand Python tooling (the `algopy` CLI), installed via `pip install algorand-python`.

Quick start:
1. Create and activate a virtual environment (example using venv):
   - python3 -m venv .venv
   - source .venv/bin/activate  (Windows: .venv\\Scripts\\activate)
2. Install dependencies:
   - pip install -r requirements.txt
3. Compile TEAL artifacts using Algorand Python:
   - python src/compile.py
   (Alternatively, directly run: `python -m algopy compile dapp/src/app.py --out dapp/build`)

Expected output:
- dapp/build/approval.teal
- dapp/build/clear.teal
- dapp/build/contract.json (ARC-4 contract spec, if supported by your algopy version)

Notes:
- This is a skeleton suitable for extension: add state variables, more methods, and deployment tooling as needed.
- The compile step is local; no Algorand node is required to generate TEAL.
- No changes were made to the Rust code; this folder is isolated.
