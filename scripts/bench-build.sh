#!/bin/sh
# Times a clean build of everything the checks compile, then the rebuild
# after an edit to kfbox-core and after an edit to the app crate.
#
#   scripts/bench-build.sh [checkout] [target-dir]
#
# The edits are a public constant appended and then removed again, so the
# checkout is left as it was. Cargo decides freshness by content, and a
# comment changes nothing the crates that depend on it see, so neither a bare
# `touch` nor a comment measures a real rebuild. SKIP_CLEAN=1 skips the clean
# build, for a target directory that is already built.
set -eu
checkout=${1:-.}
target=${2:-$(mktemp -d)/target}
cd "$checkout"
export CARGO_TARGET_DIR="$target"
seconds() {
  start=$(date +%s.%N 2>/dev/null || python3 -c 'import time;print(time.time())')
  "$@" >/dev/null 2>&1
  end=$(date +%s.%N 2>/dev/null || python3 -c 'import time;print(time.time())')
  python3 -c "print(f'{$end - $start:.1f}')"
}
edit() {
  printf '\n#[doc(hidden)]\n#[allow(dead_code)]\npub const BENCH_EDIT: u64 = %s;\n' "$(date +%s)" >>"$1"
}
undo() {
  # Drops the lines edit() added.
  python3 - "$1" <<'PY'
import sys
p = sys.argv[1]
lines = open(p).read().split("\n")
while lines and (lines[-1] == "" or lines[-1].startswith(("pub const BENCH_EDIT", "#[doc(hidden)]", "#[allow(dead_code)]"))):
    lines.pop()
open(p, "w").write("\n".join(lines) + "\n")
PY
}
cargo fetch -q
if [ "${SKIP_CLEAN:-}" != 1 ]; then
  echo "clean (cargo test --workspace --no-run): $(seconds cargo test --workspace --no-run)s"
fi
edit crates/kfbox-core/src/lib.rs
echo "after a kfbox-core edit (same): $(seconds cargo test --workspace --no-run)s"
undo crates/kfbox-core/src/lib.rs
edit src-tauri/src/lib.rs
echo "after an app edit (cargo build -p kfbox): $(seconds cargo build -p kfbox)s"
undo src-tauri/src/lib.rs
cargo test --workspace --no-run -q >/dev/null 2>&1
echo "target size: $(du -sh "$target" | cut -f1)"
