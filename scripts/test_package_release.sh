#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf -- "$WORK_DIR"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }

# An isolated fixture avoids touching the project's target/dist or invoking Cargo.
PROJECT="$WORK_DIR/project with spaces"
mkdir -p "$PROJECT/scripts" "$WORK_DIR/bin" "$WORK_DIR/caller"
cp "$ROOT_DIR/scripts/package_release.sh" "$PROJECT/scripts/"
printf 'fixture readme\n' > "$PROJECT/README.md"
printf 'fixture license\n' > "$PROJECT/LICENSE"

cat > "$WORK_DIR/bin/cargo" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[[ "$PWD" == "$EXPECTED_ROOT" ]] || exit 20
[[ "$#" == 7 ]] || exit 21
[[ "$1" == build && "$2" == --locked && "$3" == --release ]] || exit 22
[[ "$4" == --bin && "$5" == gpdfview && "$6" == --target-dir ]] || exit 23
[[ "$7" == "$EXPECTED_ROOT/target" ]] || exit 24
printf '%s\n' "$@" > "$CARGO_CALL_LOG"
# A failed build must not package even a stale executable.
if [[ "${MOCK_CARGO_FAIL:-0}" == 1 ]]; then exit 42; fi
mkdir -p "$7/release"
printf 'mock executable\n' > "$7/release/gpdfview"
chmod +x "$7/release/gpdfview"
MOCK
chmod +x "$WORK_DIR/bin/cargo"
export PATH="$WORK_DIR/bin:$PATH"
export EXPECTED_ROOT="$PROJECT"
export CARGO_CALL_LOG="$WORK_DIR/cargo-call.log"
# Deliberately conflicting: the script must use its explicit target directory.
export CARGO_TARGET_DIR="$WORK_DIR/wrong target"
cd "$WORK_DIR/caller"

check_bundle() {
    local destination="$1"
    local archive="$destination/gpdfview-linux-$(uname -m).tar.gz"
    [[ -f "$archive" ]] || fail "archive missing: $archive"
    [[ -x "$destination/gpdfview" ]] || fail "binary not executable"
    [[ "$(tar -tzf "$archive")" == $'gpdfview\nREADME.md\nLICENSE' ]] || fail "unexpected archive contents"
    mkdir -p "$WORK_DIR/extracted"
    tar -xzf "$archive" -C "$WORK_DIR/extracted"
    cmp "$PROJECT/target/release/gpdfview" "$WORK_DIR/extracted/gpdfview"
    cmp "$PROJECT/README.md" "$WORK_DIR/extracted/README.md"
    cmp "$PROJECT/LICENSE" "$WORK_DIR/extracted/LICENSE"
}

DIST_DIR='relative output/release' bash "$PROJECT/scripts/package_release.sh"
check_bundle "$WORK_DIR/caller/relative output/release"
[[ ! -e "$PROJECT/relative output" ]] || fail "relative destination resolved after cd"
[[ ! -e "$CARGO_TARGET_DIR" ]] || fail "CARGO_TARGET_DIR overrode explicit target"
echo 'PASS: relative destination, spaces, --locked, explicit target-dir, architecture and archive contents'

DIST_DIR="$WORK_DIR/absolute output" bash "$PROJECT/scripts/package_release.sh"
check_bundle "$WORK_DIR/absolute output"
echo 'PASS: absolute destination'

# Do not let an inherited DIST_DIR prevent testing the default.
env -u DIST_DIR bash "$PROJECT/scripts/package_release.sh"
check_bundle "$PROJECT/dist"
echo 'PASS: default destination'

if MOCK_CARGO_FAIL=1 DIST_DIR="$WORK_DIR/failed output" bash "$PROJECT/scripts/package_release.sh"; then
    fail 'failed Cargo build reported success'
fi
[[ ! -e "$WORK_DIR/failed output/gpdfview" ]] || fail 'stale binary copied after failure'
[[ ! -e "$WORK_DIR/failed output/gpdfview-linux-$(uname -m).tar.gz" ]] || fail 'archive produced after failure'
echo 'PASS: failed build stops packaging (even with a stale binary)'
