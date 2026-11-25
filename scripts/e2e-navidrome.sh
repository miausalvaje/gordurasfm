#!/usr/bin/env bash
# Prueba de principio a fin con un Navidrome real (sin Docker):
#   Navidrome (ListenBrainz.BaseURL → GordurasFM) → /1/submit-listens → BD de GordurasFM.
# Uso: scripts/e2e-navidrome.sh [ruta-al-binario-navidrome] [ruta-al-binario-gordurasfm]
set -euo pipefail

ND_BIN=${1:-$HOME/navidrome-pruebas/bin/navidrome}
GFM_BIN=${2:-$(dirname "$0")/../server/target/debug/gordurasfm}
WORK=$(mktemp -d)
ND_PORT=14533
GFM_PORT=18090
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$WORK"' EXIT

echo "== música de prueba"
mkdir -p "$WORK/music" "$WORK/nd" "$WORK/gfm"
for i in 1 2 3; do
  ffmpeg -loglevel error -f lavfi -i "sine=frequency=$((300 + i * 100)):duration=200" -c:a libmp3lame -b:a 64k \
    -metadata title="Canción $i" -metadata artist="Grupo de Prueba" -metadata album="Disco de Prueba" \
    -metadata album_artist="Grupo de Prueba" -metadata track="$i" "$WORK/music/$i.mp3"
done

echo "== GordurasFM en :$GFM_PORT"
GFM_LISTEN=127.0.0.1:$GFM_PORT GFM_DATA_DIR="$WORK/gfm" GFM_NAVIDROME_URL=http://127.0.0.1:$ND_PORT \
  GFM_LISTENBRAINZ_API_URL=http://127.0.0.1:9 RUST_LOG=info "$GFM_BIN" >"$WORK/gfm.log" 2>&1 &

echo "== Navidrome en :$ND_PORT"
ND_MUSICFOLDER="$WORK/music" ND_DATAFOLDER="$WORK/nd" ND_PORT=$ND_PORT ND_ADDRESS=127.0.0.1 \
  ND_LISTENBRAINZ_ENABLED=true ND_LISTENBRAINZ_BASEURL="http://127.0.0.1:$GFM_PORT/1/" \
  ND_LOGLEVEL=info ND_SCANNER_SCHEDULE=0 "$ND_BIN" >"$WORK/nd.log" 2>&1 &

for _ in $(seq 60); do curl -sf "http://127.0.0.1:$ND_PORT/ping" >/dev/null && curl -sf "http://127.0.0.1:$GFM_PORT/api/v1/salud" >/dev/null && break; sleep 0.5; done

echo "== admin de Navidrome y escaneo"
curl -sf -X POST "http://127.0.0.1:$ND_PORT/auth/createAdmin" -H 'Content-Type: application/json' \
  -d '{"username":"pablo","password":"prueba123"}' >/dev/null
JWT=$(curl -sf -X POST "http://127.0.0.1:$ND_PORT/auth/login" -H 'Content-Type: application/json' \
  -d '{"username":"pablo","password":"prueba123"}' | jq -r .token)
SUB="u=pablo&p=prueba123&v=1.16.1&c=e2e&f=json"
curl -sf "http://127.0.0.1:$ND_PORT/rest/startScan?$SUB" >/dev/null
for _ in $(seq 60); do
  n=$(curl -sf "http://127.0.0.1:$ND_PORT/rest/search3?$SUB&query=&songCount=10" | jq '.["subsonic-response"].searchResult3.song | length')
  [ "$n" = "3" ] && break; sleep 0.5
done
echo "   canciones en Navidrome: $n"

echo "== login en GordurasFM con la cuenta de Navidrome"
CJ="$WORK/cookies"
curl -sf -c "$CJ" -X POST "http://127.0.0.1:$GFM_PORT/api/v1/auth/login" -H 'X-Requested-With: e2e' \
  -H 'Content-Type: application/json' -d '{"username":"pablo","password":"prueba123"}' | jq -c .user
TOKEN=$(curl -sf -b "$CJ" -X POST "http://127.0.0.1:$GFM_PORT/api/v1/tokens" -H 'X-Requested-With: e2e' \
  -H 'Content-Type: application/json' -d '{"kind":"scrobble","label":"Navidrome"}' | jq -r .token)

echo "== enlazar el token en el perfil de Navidrome (como en Ajustes → ListenBrainz)"
curl -sf -X PUT "http://127.0.0.1:$ND_PORT/api/listenbrainz/link" -H "X-ND-Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' -d "{\"token\":\"$TOKEN\"}" | jq -c .

echo "== reproducir: now playing + scrobble de 2 canciones"
IDS=$(curl -sf "http://127.0.0.1:$ND_PORT/rest/search3?$SUB&query=&songCount=10" | jq -r '.["subsonic-response"].searchResult3.song[].id' | head -2)
T=$(( $(date +%s) * 1000 - 600000 ))
for id in $IDS; do
  curl -sf "http://127.0.0.1:$ND_PORT/rest/scrobble?$SUB&id=$id&submission=false" >/dev/null
  curl -sf "http://127.0.0.1:$ND_PORT/rest/scrobble?$SUB&id=$id&submission=true&time=$T" >/dev/null
  T=$(( T + 210000 ))
done

echo "== esperar a que lleguen a GordurasFM"
for _ in $(seq 40); do
  total=$(curl -sf -b "$CJ" "http://127.0.0.1:$GFM_PORT/api/v1/scrobbles" | jq .total)
  [ "$total" = "2" ] && break; sleep 0.5
done
curl -sf -b "$CJ" "http://127.0.0.1:$GFM_PORT/api/v1/scrobbles" | jq -c '.items[] | {track, artist, album, duration_ms, source}'
echo "--- registro de GordurasFM"
grep -E "escuchas|usuario nuevo|ERROR|WARN" "$WORK/gfm.log" | sed 's/\x1b\[[0-9;]*m//g' | tail -8
echo "--- registro de Navidrome (ListenBrainz)"
grep -i listenbrainz "$WORK/nd.log" | tail -5 || true

if [ "$total" = "2" ]; then echo "E2E OK: 2 escuchas de Navidrome guardadas"; else echo "E2E FALLO: total=$total"; exit 1; fi
