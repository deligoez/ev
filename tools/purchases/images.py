#!/usr/bin/env python3
"""Hang each line's product images on it: purchase NDJSON in, the same with `image` lines out.

An adapter's output passes through unchanged; after each `purchase` line come `image`
attachments for the pictures saved under ~/.ev/purchases/<source>/raw/images/ for that line.
A picture belongs to a line when its file name (without extension) is the line's `sku` or `key`
(`:` in a key read as `-line-` or `-`), or starts with one of them and a hyphen:
`<sku>-01.jpg`, `<sku>.jpg`, `<order>-line-<item>.jpg`. When a picture is kept both whole
(`-orig`) and scaled (`-x5`), the whole one is enough. `ev buy bring` makes them documents of
kind `image` on the thing, never its photos. Usage:

  tools/purchases/hepsiburada.py | tools/purchases/images.py | ev buy import --stdin
"""
import argparse
import json
import re
import sys
from pathlib import Path

SCALED = re.compile(r"-x\d+$")


def names(line: dict) -> list[str]:
    """What a picture of this line is named after."""
    out = []
    for v in (line.get("sku"), line.get("key")):
        if v is None:
            continue
        v = str(v).strip()
        if not v:
            continue
        out.append(v)
        if ":" in v:
            out += [v.replace(":", "-line-"), v.replace(":", "-")]
    return out


def pictures(folder: Path, line: dict, index: dict) -> list[Path]:
    """The pictures in `folder` that belong to `line`, by name, in order."""
    found = []
    for n in names(line):
        found += index.get(n, [])
        found += [p for stem, ps in index.items() if stem.startswith(n + "-") for p in ps]
    seen, unique = set(), []
    for p in sorted(found):
        if p not in seen:
            seen.add(p)
            unique.append(p)
    whole = {SCALED.sub("", p.stem.removesuffix("-orig")) for p in unique if p.stem.endswith("-orig")}
    return [p for p in unique if not (SCALED.search(p.stem) and SCALED.sub("", p.stem) in whole)]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default="~/.ev/purchases", help="where each shop's folder is")
    args = ap.parse_args()
    root = Path(args.root).expanduser()
    indexes: dict[str, dict] = {}
    for raw in sys.stdin:
        raw = raw.strip()
        if not raw:
            continue
        print(raw)
        line = json.loads(raw)
        if line.get("type", "purchase") != "purchase":
            continue
        source = line.get("source")
        folder = root / str(source) / "raw" / "images"
        if source not in indexes:
            index: dict[str, list[Path]] = {}
            if folder.is_dir():
                for p in folder.iterdir():
                    if p.is_file():
                        index.setdefault(p.stem, []).append(p)
            indexes[source] = index
        for p in pictures(folder, line, indexes[source]):
            print(json.dumps({"type": "image", "source": source, "purchase": line["key"],
                              "file": str(p)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
