"""Scores where ev would put every placed item against a reviewed key; see README.md."""
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

data = Path(os.environ.get("EV_MEASURE", Path.home() / ".ev" / "measure"))
key = json.load(open(data / "placement-key.json", encoding="utf-8"))
moves, excluded = key["moves"], key["excluded"]


def label(n):
    return f'{n.get("code") or ""} {n["name"]}'


def misses(ev, db):
    env = dict(os.environ, EV_DB=db)
    run = lambda *a: json.loads(subprocess.run([ev, *a], env=env, capture_output=True, text=True, check=True).stdout)
    items = []

    def walk(n, parent):
        if n.get("kind") == "item" and parent is not None:
            items.append((n, parent))
        for c in n.get("children", []):
            walk(c, n)

    for r in run("tree", "--json")["tree"]:
        walk(r, None)

    def best(pair):
        item, parent = pair
        similar = run("suggest", "--json", "--for", str(item["id"])).get("similar") or []
        if similar and similar[0]["container"]["id"] != parent["id"]:
            return item["name"], label(similar[0]["container"])
        return None

    with ThreadPoolExecutor(8) as pool:
        return len(items), dict(m for m in pool.map(best, items) if m)


