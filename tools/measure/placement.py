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


