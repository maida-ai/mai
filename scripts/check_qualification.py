#!/usr/bin/env python3
"""Bounded locked dependency/license and workflow policy checks (stdlib only)."""
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
# Exact reviewed SPDX alternatives: each expression offers a permissive option.
# A changed expression requires review; this is not a license text/legal audit.
LICENSES = {
    "MIT", "Apache-2.0", "MIT OR Apache-2.0", "Apache-2.0 OR MIT",
    "MIT/Apache-2.0", "Unlicense OR MIT", "Zlib",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
    "MIT OR Apache-2.0 OR LGPL-2.1-or-later",
}


def check():
    metadata = json.loads(subprocess.run(
        ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, text=True, timeout=90,
    ).stdout)
    for package in metadata["packages"]:
        if package["license"] not in LICENSES:
            raise ValueError(f"review license for {package['name']} {package['version']}: {package['license']!r}")
        if package["source"] is not None and not package["source"].startswith("registry+"):
            raise ValueError(f"review non-registry dependency {package['name']}")
    for path in (ROOT / ".github/workflows").glob("*.yml"):
        text = path.read_text()
        if path.name.startswith("experiment-"):
            events = text.split("on:\n", 1)[1].split("\npermissions:", 1)[0].split("\njobs:", 1)[0]
            if "workflow_dispatch:" not in events or re.search(r"^  (push|pull_request|schedule|workflow_run):", events, re.M):
                raise ValueError(f"{path.name}: experiments must remain manual-only")
        timeouts = re.findall(r"timeout-minutes: (\d+)", text)
        if "runs-on:" in text and (not timeouts or any(not 1 <= int(t) <= 20 for t in timeouts)):
            raise ValueError(f"{path.name}: runner jobs need a timeout of 1-20 minutes")
        if "upload-artifact@" in text and not re.search(r"retention-days: (?:[1-9]|[12][0-9]|30)\s*$", text, re.M):
            raise ValueError(f"{path.name}: artifacts need retention of 1-30 days")
    print(f"Reviewed licenses/sources for {len(metadata['packages'])} locked packages; bounded workflow policies passed")


if __name__ == "__main__":
    try:
        check()
    except (ValueError, KeyError, IndexError, subprocess.SubprocessError) as error:
        print(f"Qualification check failed: {error}. Fetch locked metadata with `cargo fetch --locked` if offline metadata is missing; review changed licenses/workflows explicitly.", file=sys.stderr)
        sys.exit(1)
