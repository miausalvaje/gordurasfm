#!/data/data/com.termux/files/usr/bin/bash
# Instala o actualiza GordurasFM en Termux (Android, aarch64), como servicio de termux-services.
#
#   curl -fsSL https://raw.githubusercontent.com/miausalvaje/gordurasfm/main/deploy/termux/install.sh | bash
#   curl -fsSL …/install.sh | bash -s -- --version v0.1.0 --navidrome-url http://127.0.0.1:4533
#
# Opciones: --version vX.Y.Z · --binary RUTA · --navidrome-url URL · --port N (8090)
# Guía completa: docs/TERMUX.md
set -euo pipefail

REPO=${GFM_REPO:-miausalvaje/gordurasfm}
RELEASES=${GFM_RELEASES_URL:-https://github.com/$REPO/releases}
PREFIX=${PREFIX:-/data/data/com.termux/files/usr}
HOME_DIR=${GFM_HOME:-$HOME/.gordurasfm}
CONFIG=$HOME_DIR/config.toml
SV=$PREFIX/var/service/gordurasfm

VERSION="" BINARY="" NAVIDROME_URL="http://127.0.0.1:4533" PORT=8090
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION=$2; shift 2 ;;
    --binary) BINARY=$2; shift 2 ;;
    --navidrome-url) NAVIDROME_URL=$2; shift 2 ;;
    --port) PORT=$2; shift 2 ;;
    *) echo "opción desconocida: $1" >&2; exit 2 ;;
  esac
done
say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
die() { printf '\033[31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

[ "$(uname -m)" = aarch64 ] || die "solo hay binario para aarch64 (tu móvil es $(uname -m))"
if ! command -v sv-enable >/dev/null || ! command -v curl >/dev/null; then
  say "Instalando termux-services y curl"
  pkg install -y termux-services curl
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
if [ -n "$BINARY" ]; then
  cp "$BINARY" "$TMP/gordurasfm"
else
  if [ -z "$VERSION" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)
    [ -n "$VERSION" ] || die "no se ha podido saber la última versión"
  fi
  ASSET=gordurasfm-aarch64-linux-musl
  say "Descargando GordurasFM $VERSION"
  curl -fsSL --retry 3 -o "$TMP/$ASSET" "$RELEASES/download/$VERSION/$ASSET"
  curl -fsSL --retry 3 -o "$TMP/SHA256SUMS" "$RELEASES/download/$VERSION/SHA256SUMS"
  (cd "$TMP" && grep " $ASSET\$" SHA256SUMS | sha256sum -c --quiet -) || die "la suma SHA-256 no coincide"
  mv "$TMP/$ASSET" "$TMP/gordurasfm"
fi
chmod 755 "$TMP/gordurasfm"
"$TMP/gordurasfm" version >/dev/null || die "el binario no se ejecuta"

mkdir -p "$HOME_DIR/data"
if [ -x "$PREFIX/bin/gordurasfm" ] && [ -f "$HOME_DIR/data/gordurasfm.db" ]; then
  say "Copia de la BD antes de actualizar"
  "$PREFIX/bin/gordurasfm" backup --config "$CONFIG"
fi
install -m 755 "$TMP/gordurasfm" "$PREFIX/bin/gordurasfm"

if [ ! -f "$CONFIG" ]; then
  say "Creando $CONFIG"
  cat > "$CONFIG" <<EOF
# Configuración de GordurasFM en Termux. Todas las opciones:
# https://github.com/$REPO/blob/main/server/config.example.toml
listen = "0.0.0.0:$PORT"
data_dir = "$HOME_DIR/data"
public_url = "http://127.0.0.1:$PORT"
default_timezone = "Europe/Madrid"

[navidrome]
url = "$NAVIDROME_URL"
EOF
  chmod 600 "$CONFIG"
fi

say "Servicio (termux-services)"
mkdir -p "$SV/log"
cat > "$SV/run" <<EOF
#!$PREFIX/bin/sh
# Evita que Android duerma la CPU mientras GordurasFM está en marcha.
termux-wake-lock 2>/dev/null || true
exec $PREFIX/bin/gordurasfm serve --config $CONFIG 2>&1
EOF
chmod 755 "$SV/run"
ln -sf "$PREFIX/share/termux-services/svlogger" "$SV/log/run"
# termux-services arranca con la sesión de Termux; si es la primera vez, hay que reiniciar Termux.
if command -v sv >/dev/null && [ -n "${SVDIR:-}" ]; then
  sv-enable gordurasfm >/dev/null 2>&1 || true
  sv restart gordurasfm >/dev/null 2>&1 || sv up gordurasfm
  sleep 3
  if gordurasfm healthcheck --config "$CONFIG" >/dev/null 2>&1; then
    say "GordurasFM funcionando en http://127.0.0.1:$PORT"
  else
    die "no responde; mira: tail $PREFIX/var/log/sv/gordurasfm/current"
  fi
else
  say "Cierra Termux del todo y vuelve a abrirlo; después: sv-enable gordurasfm"
fi
cat <<EOF

Importante en Android:
  - Quita la optimización de batería a Termux (Ajustes → Apps → Termux → Batería → Sin restricciones).
  - Para que arranque al encender el móvil, instala Termux:Boot (mira docs/TERMUX.md).
  - En tu Navidrome: ListenBrainz.BaseURL = "http://127.0.0.1:$PORT/1/"
EOF
