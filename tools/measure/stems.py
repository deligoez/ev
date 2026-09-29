"""Scores Lexicon::key against a hand-judged list of stems; see README.md."""
import os
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parents[2]
