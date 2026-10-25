#!/usr/bin/env bash
# Build everything from source and assemble an installable bundle in dist/beacn-suite/.
#   beacn-utility (this repo), beacn-mixer (./beacn-mixer), and Pipeweaver (upstream at a
#   pinned commit with patches/pipeweaver/*.patch applied).
# Usage: packaging/build.sh            Needs: rust/cargo, npm, git and the -devel packages
#        listed in README.md (or build inside contrib/Containerfile). No sudo.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PIPEWEAVER_REPO="${PIPEWEAVER_REPO:-https://github.com/pipeweaver/pipeweaver}"
PIPEWEAVER_REV="${PIPEWEAVER_REV:-006b04514ab4241fb2b7e845c33e743ecdf27977}"
WORK="$ROOT/build"
OUT="$ROOT/dist/beacn-suite"
mkdir -p "$WORK"

echo "== beacn-utility"
( cd "$ROOT" && cargo build --release --locked )

echo "== beacn-mixer"
( cd "$ROOT/beacn-mixer" && cargo build --release --locked )

echo "== pipeweaver @ ${PIPEWEAVER_REV:0:7} + patches"
if [ ! -d "$WORK/pipeweaver/.git" ]; then git clone "$PIPEWEAVER_REPO" "$WORK/pipeweaver"; fi
( cd "$WORK/pipeweaver"
  git fetch -q origin
  git checkout -q --force "$PIPEWEAVER_REV"
  git clean -fdq -e target
  for p in "$ROOT"/patches/pipeweaver/*.patch; do git apply --index "$p"; done
  cargo build --release -p pipeweaver-daemon -p pipeweaver-client )

echo "== Assembling $OUT"
rm -rf "$OUT"; mkdir -p "$OUT/files"
install -m755 "$ROOT/target/release/beacn-utility" "$OUT/files/beacn-utility"
install -m755 "$ROOT/beacn-mixer/target/release/beacn-mixer" "$OUT/files/beacn-mixer"
install -m755 "$WORK/pipeweaver/target/release/pipeweaver-daemon" "$OUT/files/pipeweaver-daemon"
install -m755 "$WORK/pipeweaver/target/release/pipeweaver-client" "$OUT/files/pipeweaver-client"
install -m644 "$ROOT/resources/icons/beacn-utility-large.png" "$OUT/files/io.github.beacn_on_linux.beacn-utility.png"
install -m644 "$ROOT/packaging/beacn.service" "$ROOT/packaging/io.github.beacn_on_linux.beacn-utility.desktop" "$OUT/files/"
install -m755 "$ROOT/packaging/install.sh" "$ROOT/packaging/rollback.sh" "$OUT/"
mkdir -p "$OUT/packaging"
install -m755 "$ROOT/packaging/native-mixer-install.sh" "$ROOT/packaging/native-mixer-rollback.sh" "$OUT/packaging/"
install -m644 "$ROOT/packaging/io.github.beacn_on_linux.beacn-utility.desktop" "$OUT/packaging/"
( cd "$OUT/files" && sha256sum beacn-utility beacn-mixer pipeweaver-daemon pipeweaver-client > "$OUT/SHA256SUMS" )
echo "Built. Next: (cd $OUT && ./install.sh)"
