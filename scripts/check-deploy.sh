#!/usr/bin/env bash
# Comprobaciones de los archivos de despliegue (se lanza en la CI).
set -euo pipefail
cd "$(dirname "$0")/.."
# La unidad que escribe el instalador debe ser igual que deploy/systemd/gordurasfm.service.
diff -u deploy/systemd/gordurasfm.service \
  <(sed -n "/^cat > \"\$UNIT\" <<'UNIT_EOF'\$/,/^UNIT_EOF\$/p" deploy/lxc/install.sh | sed '1d;$d') \
  || { echo "la unidad de deploy/lxc/install.sh no coincide con deploy/systemd/gordurasfm.service" >&2; exit 1; }
for f in deploy/lxc/install.sh deploy/termux/install.sh scripts/*.sh; do bash -n "$f"; done
if command -v shellcheck >/dev/null; then
  shellcheck -S warning deploy/lxc/install.sh deploy/termux/install.sh scripts/build-release.sh scripts/check-deploy.sh
fi
if command -v systemd-analyze >/dev/null; then
  systemd-analyze verify deploy/systemd/gordurasfm.service 2>&1 | grep -v "gordurasfm\b.*No such file\|Command .* is not executable" || true
fi
echo "despliegue: ok"
