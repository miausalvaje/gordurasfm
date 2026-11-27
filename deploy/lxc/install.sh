#!/usr/bin/env bash
# OWNFM_INSTALLER — instala o actualiza OwnFM en Debian/Ubuntu con systemd (también en un LXC
# sin privilegios de Proxmox). Volver a lanzarlo actualiza: guarda una copia de la BD antes de
# cambiar el binario y, si el nuevo no arranca, vuelve al anterior.
#
#   curl -fsSL https://raw.githubusercontent.com/miausalvaje/ownfm/main/deploy/lxc/install.sh \
#     | sudo bash -s -- --navidrome-url http://192.168.1.20:4533
#
# Opciones:
#   --version vX.Y.Z      versión publicada en GitHub (por defecto, la última)
#   --binary RUTA         usar un binario ya descargado o compilado
#   --navidrome-url URL   Navidrome (solo al instalar; luego se cambia en /etc/ownfm/config.toml)
#   --public-url URL      URL con la que se entra desde el navegador (por defecto http://<IP>:<puerto>)
#   --port N              puerto (8090)
#   --no-start            instalar sin arrancar
#
# Después, para actualizar: sudo ownfm-update [--version vX.Y.Z | --binary RUTA]
set -euo pipefail

REPO=${OWNFM_REPO:-miausalvaje/ownfm}
RELEASES=${OWNFM_RELEASES_URL:-https://github.com/$REPO/releases}
BIN=/usr/local/bin/ownfm
ETC=/etc/ownfm
CONFIG=$ETC/config.toml
DATA=/var/lib/ownfm
UNIT=/etc/systemd/system/ownfm.service
UPDATER=/usr/local/sbin/ownfm-update
USER_NAME=ownfm

VERSION="" BINARY="" NAVIDROME_URL="" PUBLIC_URL="" PORT=8090 START=1
while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION=$2; shift 2 ;;
    --binary) BINARY=$2; shift 2 ;;
    --navidrome-url) NAVIDROME_URL=$2; shift 2 ;;
    --public-url) PUBLIC_URL=$2; shift 2 ;;
    --port) PORT=$2; shift 2 ;;
    --no-start) START=0; shift ;;
    -h|--help) sed -n '2,18p' "$0" 2>/dev/null | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "opción desconocida: $1" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
die() { printf '\033[31mERROR: %s\033[0m\n' "$*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "hay que ejecutarlo como root (sudo)"
command -v systemctl >/dev/null || die "este instalador necesita systemd (para Docker o Termux, mira la documentación)"

case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) die "arquitectura no soportada: $(uname -m) (hay binarios para x86_64 y aarch64)" ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fetch() { # fetch URL DESTINO
  if command -v curl >/dev/null; then curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null; then wget -q -O "$2" "$1"
  else die "hace falta curl o wget"; fi
}

# 1. Binario
if [ -n "$BINARY" ]; then
  [ -f "$BINARY" ] || die "no existe $BINARY"
  cp "$BINARY" "$TMP/ownfm"
else
  if [ -z "$VERSION" ]; then
    say "Buscando la última versión"
    fetch "https://api.github.com/repos/$REPO/releases/latest" "$TMP/latest.json"
    VERSION=$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$TMP/latest.json" | head -1)
    [ -n "$VERSION" ] || die "no se ha podido saber la última versión"
  fi
  ASSET="ownfm-$ARCH-linux-musl"
  say "Descargando OwnFM $VERSION ($ARCH)"
  fetch "$RELEASES/download/$VERSION/$ASSET" "$TMP/$ASSET"
  fetch "$RELEASES/download/$VERSION/SHA256SUMS" "$TMP/SHA256SUMS"
  (cd "$TMP" && grep " $ASSET\$" SHA256SUMS | sha256sum -c --quiet -) || die "la suma SHA-256 no coincide"
  mv "$TMP/$ASSET" "$TMP/ownfm"
fi
chmod 755 "$TMP/ownfm"
NEW_VERSION=$("$TMP/ownfm" version | awk '{print $NF}') || die "el binario no se ejecuta en esta máquina"

UPDATE=0
[ -x "$BIN" ] && UPDATE=1

# 1 bis. Instalación anterior con el nombre GordurasFM: se pasa a OwnFM conservando datos, clave y
# configuración. La copia de la BD la hace el binario antiguo antes de tocar nada.
MIGRATED=0
OLD_BIN=/usr/local/bin/gordurasfm
if [ "$UPDATE" = 0 ] && [ -x "$OLD_BIN" ]; then
  say "Instalación de GordurasFM encontrada: se pasa a OwnFM conservando los datos"
  if [ -f /var/lib/gordurasfm/gordurasfm.db ]; then
    runuser -u gordurasfm -- "$OLD_BIN" backup --config /etc/gordurasfm/config.toml \
      || die "no se ha podido copiar la BD; no se cambia nada"
  fi
  systemctl disable --now gordurasfm >/dev/null 2>&1 || true
  [ -d /etc/gordurasfm ] && [ ! -e "$ETC" ] && mv /etc/gordurasfm "$ETC"
  [ -d /var/lib/gordurasfm ] && [ ! -e "$DATA" ] && mv /var/lib/gordurasfm "$DATA"
  if id gordurasfm >/dev/null 2>&1 && ! id "$USER_NAME" >/dev/null 2>&1; then
    usermod -l "$USER_NAME" -d "$DATA" gordurasfm
    if getent group gordurasfm >/dev/null; then groupmod -n "$USER_NAME" gordurasfm; fi
  fi
  # Rutas de la configuración. La BD (gordurasfm.db) la renombra OwnFM al arrancar.
  [ -f "$CONFIG" ] && sed -i 's#/etc/gordurasfm#/etc/ownfm#g; s#/var/lib/gordurasfm#/var/lib/ownfm#g; s#GordurasFM#OwnFM#g; s#gordurasfm#ownfm#g' "$CONFIG"
  rm -f "$OLD_BIN" /etc/systemd/system/gordurasfm.service /usr/local/sbin/gordurasfm-update
  rm -rf /etc/systemd/system/gordurasfm.service.d
  systemctl daemon-reload
  MIGRATED=1
fi
if [ "$UPDATE" = 1 ]; then say "Actualizando OwnFM: $("$BIN" version 2>/dev/null | awk '{print $NF}') → $NEW_VERSION"; else say "Instalando OwnFM $NEW_VERSION"; fi

# 2. Usuario, carpetas, clave y configuración
if ! id "$USER_NAME" >/dev/null 2>&1; then
  useradd --system --home-dir "$DATA" --no-create-home --shell /usr/sbin/nologin "$USER_NAME"
fi
install -d -m 750 -o "$USER_NAME" -g "$USER_NAME" "$DATA"
install -d -m 750 -o root -g "$USER_NAME" "$ETC"

if [ ! -f "$ETC/master.key" ]; then
  say "Creando la clave maestra ($ETC/master.key)"
  (umask 077; head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' > "$ETC/master.key")
  chown "$USER_NAME:$USER_NAME" "$ETC/master.key"
  chmod 600 "$ETC/master.key"
fi

if [ ! -f "$CONFIG" ]; then
  IP=$(hostname -I 2>/dev/null | awk '{print $1}')
  [ -n "$PUBLIC_URL" ] || PUBLIC_URL="http://${IP:-localhost}:$PORT"
  [ -n "$NAVIDROME_URL" ] || NAVIDROME_URL="http://localhost:4533"
  say "Creando $CONFIG"
  cat > "$CONFIG" <<EOF
# Configuración de OwnFM. Todas las opciones: https://github.com/$REPO/blob/main/server/config.example.toml
listen = "0.0.0.0:$PORT"
data_dir = "$DATA"
master_key_file = "$ETC/master.key"
public_url = "$PUBLIC_URL"
default_timezone = "Europe/Madrid"
registration = "open"            # open | approval

[navidrome]
url = "$NAVIDROME_URL"

[backup]
keep_daily = 7
keep_weekly = 4
EOF
  chown root:"$USER_NAME" "$CONFIG"
  chmod 640 "$CONFIG"
fi

# 3. Copia de la BD antes de cambiar el binario
if [ "$UPDATE" = 1 ] && [ -f "$DATA/ownfm.db" ]; then
  say "Copia de la BD"
  runuser -u "$USER_NAME" -- "$BIN" backup --config "$CONFIG" || die "no se ha podido copiar la BD; no se actualiza"
fi

# 4. Binario y servicio
[ "$UPDATE" = 1 ] && cp -p "$BIN" "$BIN.anterior"
install -m 755 "$TMP/ownfm" "$BIN.nuevo"
mv -f "$BIN.nuevo" "$BIN"

# --- unidad systemd (igual que deploy/systemd/ownfm.service) ---
cat > "$UNIT" <<'UNIT_EOF'
[Unit]
Description=OwnFM (estadísticas de escucha para Navidrome)
Documentation=https://github.com/miausalvaje/ownfm
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ownfm
Group=ownfm
ExecStart=/usr/local/bin/ownfm serve --config /etc/ownfm/config.toml
Restart=on-failure
RestartSec=5
TimeoutStopSec=30
Environment=RUST_LOG=info,tower_http=warn,sqlx=warn
UMask=0027
LimitNOFILE=8192

# Aislamiento (funciona en un LXC sin privilegios de Proxmox).
NoNewPrivileges=yes
ProtectSystem=strict
ReadWritePaths=/var/lib/ownfm
ProtectHome=yes
PrivateTmp=yes
ProtectControlGroups=yes
ProtectKernelModules=yes
ProtectKernelTunables=yes
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictRealtime=yes
RestrictNamespaces=yes
RestrictSUIDSGID=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
CapabilityBoundingSet=
PrivateDevices=yes
ProtectKernelLogs=yes
ProtectClock=yes
ProtectHostname=yes
ProtectProc=invisible
ProcSubset=pid
RemoveIPC=yes
SystemCallArchitectures=native
SystemCallFilter=@system-service
SystemCallFilter=~@privileged @resources

# Tope de memoria: en reposo usa muy poco; las importaciones grandes son lo que más gasta.
MemoryMax=384M

[Install]
WantedBy=multi-user.target
UNIT_EOF
# --- fin de la unidad ---
systemctl daemon-reload

# 5. Actualizador: copia de este script (o, si llegó por curl, uno que lo descarga)
SELF=$(readlink -f "${BASH_SOURCE[0]:-}" 2>/dev/null || true)
if [ -f "$SELF" ] && grep -q OWNFM_INSTALLER "$SELF"; then
  # Si ya se está ejecutando como ownfm-update, no hay nada que copiar.
  [ "$SELF" = "$UPDATER" ] || install -m 755 "$SELF" "$UPDATER"
else
  cat > "$UPDATER" <<EOF
#!/bin/sh
# Actualiza OwnFM con el instalador más reciente. Opciones: --version vX.Y.Z | --binary RUTA
set -e
curl -fsSL https://raw.githubusercontent.com/$REPO/main/deploy/lxc/install.sh | bash -s -- "\$@"
EOF
  chmod 755 "$UPDATER"
fi

if [ "$START" = 0 ]; then
  systemctl enable ownfm >/dev/null 2>&1
  say "Instalado sin arrancar: systemctl start ownfm"
  exit 0
fi

systemctl enable ownfm >/dev/null 2>&1
systemctl restart ownfm

# 6. Comprobación (si una actualización no arranca, vuelve al binario anterior)
ok=0
health() {
  for _ in $(seq 1 30); do
    if runuser -u "$USER_NAME" -- "$BIN" healthcheck --config "$CONFIG" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  return 1
}
health && ok=1
if [ "$ok" != 1 ]; then
  journalctl -u ownfm -n 20 --no-pager >&2 || true
  if [ "$UPDATE" = 1 ] && [ -f "$BIN.anterior" ]; then
    mv -f "$BIN.anterior" "$BIN"
    systemctl restart ownfm
    if health; then
      die "la versión nueva no arranca: se ha vuelto a la anterior, que funciona. Copia de la BD en $DATA/backups"
    fi
    die "la versión nueva no arranca y la anterior tampoco (¿la nueva migró la BD?): restaura la última copia de $DATA/backups (docs/DESPLIEGUE.md, sección 5)"
  fi
  die "OwnFM no responde; mira: journalctl -u ownfm"
fi
rm -f "$BIN.anterior"

PORT_NOW=$(sed -n 's/^listen *= *"[^"]*:\([0-9]*\)".*/\1/p' "$CONFIG" | head -1)
URL=$(sed -n 's/^public_url *= *"\([^"]*\)".*/\1/p' "$CONFIG" | head -1)
say "OwnFM $NEW_VERSION funcionando en ${URL:-http://localhost:${PORT_NOW:-8090}}"
if [ "$UPDATE" = 0 ] && [ "$MIGRATED" = 0 ]; then
  cat <<EOF

Siguientes pasos:
  1. Revisa $CONFIG (sobre todo [navidrome] url) y: systemctl restart ownfm
  2. En Navidrome: ListenBrainz.Enabled = true y ListenBrainz.BaseURL = "${URL:-http://<esta-IP>:$PORT}/1/"
  3. Cada usuario entra en la web con su cuenta de Navidrome, crea un token de escucha
     y lo pega en Navidrome → Personal → ListenBrainz.
Actualizar: sudo ownfm-update · Registros: journalctl -u ownfm -f
EOF
fi
