#!/usr/bin/env python3
"""Run the same disposable SDK rehearsal shipped to application developers."""
import os
from pathlib import Path
import runpy

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
runpy.run_path(str(ROOT / "templates/rust-react/sdk/testing/stewards.py"), run_name="__main__")
