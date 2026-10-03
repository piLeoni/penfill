#!/usr/bin/env bash
# Build penfill-wasm for the browser (wasm-pack).
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

command -v wasm-pack >/dev/null || { echo "install wasm-pack: cargo install wasm-pack" >&2; exit 1; }

wasm-pack build "$root/bindings/wasm" \
  --target web \
  --out-dir "$root/web/pkg" \
  --release \
  -- --no-default-features

echo "Wrote web/pkg/"
