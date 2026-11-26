#!/usr/bin/env bash
# Compila la web Flutter (sin CDN: CanvasKit y fuentes salen del propio binario).
set -euo pipefail
cd "$(dirname "$0")/../web"
FLUTTER=${FLUTTER:-$(command -v flutter || echo "$HOME/flutter/bin/flutter")}
"$FLUTTER" build web --release --no-web-resources-cdn --no-wasm-dry-run "$@"
