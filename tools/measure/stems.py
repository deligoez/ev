"""Scores Lexicon::key against a hand-judged list of stems; see README.md."""
import os
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[2]
data = Path(os.environ.get("EV_MEASURE", Path.home() / ".ev" / "measure"))
baseline_path, judged_path = data / "stems-baseline.tsv", data / "stems-judged.tsv"


def table(path):
    return dict(l.rstrip("\n").split("\t")[:2] for l in open(path, encoding="utf-8") if l.strip())


baseline = table(baseline_path)
expected, skip = dict(baseline), set()
for line in open(judged_path, encoding="utf-8"):
    if line.startswith("#") or line.startswith("yüzey") or not line.strip():
        continue
    word, ev, other, verdict = line.rstrip("\n").split("\t")[:4]
    if verdict == "çözgü":
        expected[word] = other
    elif verdict == "ev":
        expected[word] = ev
    else:
        skip.add(word)

out = data / "stems-now.tsv"
env = dict(os.environ, EV_WORDS=str(baseline_path), EV_OUT=str(out))
subprocess.run(
    ["cargo", "test", "-q", "-p", "ev-core", "--lib", "dump_keys", "--", "--ignored"],
    cwd=root, env=env, check=True, capture_output=True,
)
