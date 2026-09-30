#!/usr/bin/env bash
# Dead code across the two crates: a scan to read, not a gate.
#
# rustc's dead_code lint skips a library's `pub` items, since another crate might use them, and
# ev-core's only user is ev-cli. This copies core/src into the cli's binary crate as a module in
# a scratch folder, so every item is private to one binary and dead_code sees the whole program.
# Tests are not compiled, so the list holds two kinds of finding: code nothing calls, and code
# only tests call (a test helper, or a test of something the product no longer does). Judge each
# one; do not add a call to make it go away.
#
# Usage: tools/deadcode.sh [scratch dir]   (from the repository root; needs perl and python3)
set -euo pipefail
E=$(pwd)
[ -f "$E/core/src/lib.rs" ] && [ -f "$E/cli/src/main.rs" ] || {
  echo "run from the ev repository root" >&2
  exit 2
}
S=${1:-$(mktemp -d)}
W="$S/wp"
rm -rf "$W" && mkdir -p "$W"
cp -R "$E/cli/src" "$W/src"
cp -R "$E/core/src" "$W/src/ev_core"
mv "$W/src/ev_core/lib.rs" "$W/src/ev_core/mod.rs"
# Inside the library, crate:: now means the binary's root.
find "$W/src/ev_core" -name '*.rs' -exec perl -pi -e 's/\bcrate::/crate::ev_core::/g' {} +
# In the binary's submodules, the path of the extern crate becomes a module path.
find "$W/src" -name '*.rs' -not -path "$W/src/ev_core/*" -not -path "$W/src/main.rs" \
  -exec perl -pi -e 's/(?<!crate::)(?<![\w:])ev_core::/crate::ev_core::/g' {} +
perl -0pi -e 's/\A/mod ev_core;\n/' "$W/src/main.rs"
# Both manifests' dependencies, ev-core itself left out, chrono's features merged.
python3 - "$E" "$W" <<'PY'
import sys, re
e, w = sys.argv[1:]
def deps(t):
    m = re.search(r'\[dependencies\]\n(.*?)(\n\[|\Z)', t, re.S)
    return [l for l in m.group(1).splitlines() if l.strip() and not l.startswith('ev-core')]
d = {}
for f in ('core', 'cli'):
    for l in deps(open(f'{e}/{f}/Cargo.toml').read()):
        d.setdefault(l.split('=')[0].strip(), l)
d['chrono'] = 'chrono = { version = "0.4", default-features = false, features = ["now", "clock"] }'
open(f'{w}/Cargo.toml', 'w').write(
    '[package]\nname = "wp"\nversion = "0.0.0"\nedition = "2024"\n[workspace]\n[dependencies]\n'
    + '\n'.join(d.values()) + '\n')
PY
cp "$E/Cargo.lock" "$W/"
cd "$W"
CARGO_TARGET_DIR="$S/target" cargo check --message-format=short 2>&1 \
  | grep -E 'never (used|read|constructed)' \
  | sed 's#^src/ev_core/#core/src/#; s#^src/#cli/src/#' || echo "no dead code found"
