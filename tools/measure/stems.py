"""Scores Lexicon::key against a hand-judged list of stems; see README.md."""
import os
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[2]
data = Path(os.environ.get("EV_MEASURE", Path.home() / ".ev" / "measure"))
baseline_path, judged_path = data / "stems-baseline.tsv", data / "stems-judged.tsv"


