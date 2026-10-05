#!/bin/sh
# A copy of an inventory to test against, so a QA round never writes to the real one.
#
#   tools/qa/sandbox.sh [<inventory dir, default ~/.ev>] [<sandbox dir, default a new temp dir>]
#
# The database is copied with SQLite's online backup (a consistent whole even while the inventory
# is in use; `cp` of ev.db alone may miss writes still in ev.db-wal). Photos and documents are
# stored by paths relative to the database, so they are copied beside it: as APFS clones where the
# file system has them (no extra space), plain copies elsewhere. The settings get a copy of their
# own, so a test that changes the language or theme leaves the person's `ev ui` alone.
#
# Prints the two variables every command of the round needs. Each agent shell starts fresh, so
# put them in front of every command (`EV_DB=… EV_CONFIG=… ev …`), or pass `--db` on each.
set -eu

src="${1:-$HOME/.ev}"
dir="${2:-$(mktemp -d "${TMPDIR:-/tmp}/ev-qa.XXXXXX")}"

if [ ! -f "$src/ev.db" ]; then
	echo "no ev.db in $src" >&2
	exit 1
fi
case "$(cd "$dir" 2>/dev/null && pwd -P)" in
"$(cd "$src" && pwd -P)")
	echo "the sandbox must not be the inventory itself" >&2
	exit 1
	;;
esac

mkdir -p "$dir"
sqlite3 "$src/ev.db" ".backup '$dir/ev.db'"
for sub in photos docs; do
	if [ -d "$src/$sub" ]; then
		cp -Rc "$src/$sub" "$dir/$sub" 2>/dev/null || cp -R "$src/$sub" "$dir/$sub"
	fi
done
if [ -f "$src/settings.json" ]; then
	cp "$src/settings.json" "$dir/settings.json"
fi

printf 'EV_DB=%s EV_CONFIG=%s\n' "$dir/ev.db" "$dir/settings.json"
