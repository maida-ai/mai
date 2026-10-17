#!/usr/bin/env python3
"""Collect bounded top-level product evidence; never copy fixture source/caches."""
import argparse
import gzip
import hashlib
from pathlib import Path
import shutil


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    destination = args.destination.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    for item in sorted(source.iterdir()):
        if not item.is_file() or item.suffix not in (".log", ".json", ".jsonl", ".txt"):
            continue
        if item.suffix == ".log":
            with item.open("rb") as original, (destination/(item.name+".gz")).open("wb") as compressed:
                with gzip.GzipFile(fileobj=compressed, mode="wb", mtime=0) as stream:
                    shutil.copyfileobj(original, stream)
        else:
            shutil.copyfile(item, destination/item.name)
    lines = [hashlib.sha256(p.read_bytes()).hexdigest()+"  "+p.name for p in sorted(destination.iterdir())]
    (destination/"SHA256SUMS").write_text("\n".join(lines)+"\n")


if __name__ == "__main__":
    main()
