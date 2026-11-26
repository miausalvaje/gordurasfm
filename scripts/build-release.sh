#!/usr/bin/env bash
# Binarios de publicación: web Flutter + servidor estático (musl) para x86_64 y aarch64 → dist/
#
#   dist/gordurasfm-x86_64-linux-musl     LXC / servidores / Docker amd64
#   dist/gordurasfm-aarch64-linux-musl    Termux (Android), Raspberry Pi, Docker arm64
#   dist/SHA256SUMS
#   dist/linux/{amd64,arm64}/gordurasfm   lo que copia el Dockerfile
#
# Necesita Flutter, Rust con los targets musl, zig y cargo-zigbuild. SKIP_WEB=1 reutiliza web/build/web.
set -euo pipefail
cd "$(dirname "$0")/.."
[ "${SKIP_WEB:-0}" = 1 ] || scripts/build-web.sh
rm -rf dist
mkdir -p dist/linux/amd64 dist/linux/arm64
(
  cd server
  for t in x86_64 aarch64; do
    cargo zigbuild --release --locked --target "$t-unknown-linux-musl"
    cp "target/$t-unknown-linux-musl/release/gordurasfm" "../dist/gordurasfm-$t-linux-musl"
  done
)
cd dist
sha256sum gordurasfm-* > SHA256SUMS
cp gordurasfm-x86_64-linux-musl linux/amd64/gordurasfm
cp gordurasfm-aarch64-linux-musl linux/arm64/gordurasfm
ls -l gordurasfm-* SHA256SUMS
