#!/usr/bin/env bash
# The quality gate (CLAUDE.md) on the committed HEAD, in a worktree of its own with its own
# target directory, so work goes on in the main tree while it runs and nothing edited there
# meanwhile leaks into what is checked. Its exit status is the gate's: push only on 0.
#
#   tools/gate.sh              the HEAD of the main tree
#   EV_GATE_DIR=… tools/gate.sh  elsewhere than target/gate
set -euo pipefail

root=$(git rev-parse --show-toplevel)
dir=${EV_GATE_DIR:-$root/target/gate}
rev=$(git -C "$root" rev-parse HEAD)

if [ ! -e "$dir/.git" ]; then
	git -C "$root" worktree add --detach "$dir" "$rev" >/dev/null
fi
git -C "$dir" checkout --detach --quiet "$rev"
cd "$dir"
echo "gate on $(git rev-parse --short HEAD)"
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo shear
cargo nextest run --workspace
