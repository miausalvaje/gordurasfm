#!/usr/bin/env bash
# Prueba con datos reales: importa el historial público de un usuario de Last.fm y saca estadísticas.
# Uso: LASTFM_API_KEY=… scripts/e2e-lastfm-real.sh <usuario-lastfm> [binario-ownfm] [binario-navidrome]
# Solo usa llamadas públicas (user.getRecentTracks); no hace falta el secret.
set -euo pipefail

LFM_USER=${1:?usuario de Last.fm}
OWNFM_BIN=${2:-$(dirname "$0")/../server/target/x86_64-unknown-linux-musl/release/ownfm}
ND_BIN=${3:-$HOME/navidrome-pruebas/bin/navidrome}
: "${LASTFM_API_KEY:?falta LASTFM_API_KEY}"
WORK=$(mktemp -d)
ND_PORT=14534
OWNFM_PORT=18092
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$WORK"' EXIT

mkdir -p "$WORK/music" "$WORK/nd" "$WORK/ownfm"
ffmpeg -loglevel error -f lavfi -i "sine=frequency=440:duration=60" -c:a libmp3lame -b:a 64k -metadata title=x -metadata artist=y "$WORK/music/x.mp3"
ND_MUSICFOLDER="$WORK/music" ND_DATAFOLDER="$WORK/nd" ND_PORT=$ND_PORT ND_ADDRESS=127.0.0.1 ND_LOGLEVEL=warn "$ND_BIN" >"$WORK/nd.log" 2>&1 &
OWNFM_LISTEN=127.0.0.1:$OWNFM_PORT OWNFM_DATA_DIR="$WORK/ownfm" OWNFM_NAVIDROME_URL=http://127.0.0.1:$ND_PORT \
  OWNFM_LASTFM_API_KEY="$LASTFM_API_KEY" OWNFM_LASTFM_API_SECRET=00000000000000000000000000000000 \
  RUST_LOG=info "$OWNFM_BIN" >"$WORK/ownfm.log" 2>&1 &
OWNFM_PID=$!
for _ in $(seq 60); do curl -sf "http://127.0.0.1:$ND_PORT/ping" >/dev/null && curl -sf "http://127.0.0.1:$OWNFM_PORT/api/v1/salud" >/dev/null && break; sleep 0.5; done
curl -sf -X POST "http://127.0.0.1:$ND_PORT/auth/createAdmin" -H 'Content-Type: application/json' -d '{"username":"pablo","password":"prueba123"}' >/dev/null

CJ="$WORK/c"
H=(-H 'X-Requested-With: e2e' -H 'Content-Type: application/json')
curl -sf -c "$CJ" -X POST "http://127.0.0.1:$OWNFM_PORT/api/v1/auth/login" "${H[@]}" -d '{"username":"pablo","password":"prueba123"}' >/dev/null
echo "RSS antes: $(ps -o rss= -p $OWNFM_PID) KB"

T0=$(date +%s)
JOB=$(curl -sf -b "$CJ" -X POST "http://127.0.0.1:$OWNFM_PORT/api/v1/import/lastfm" "${H[@]}" -d "{\"mode\":\"full\",\"username\":\"$LFM_USER\"}" | jq .id)
MAXRSS=0
while :; do
  J=$(curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/import/jobs/$JOB")
  R=$(ps -o rss= -p $OWNFM_PID | tr -d ' '); [ "$R" -gt "$MAXRSS" ] && MAXRSS=$R
  S=$(echo "$J" | jq -r .status)
  [ "$S" = done ] || [ "$S" = failed ] && break
  sleep 2
done
T1=$(date +%s)
echo "Importación: $(echo "$J" | jq -c '{status, progress_done, progress_total, stats, error}') en $((T1 - T0)) s"
echo "RSS máximo durante la importación: $MAXRSS KB · después: $(ps -o rss= -p $OWNFM_PID) KB"
echo "BD: $(du -sh "$WORK/ownfm" | cut -f1)"

for p in all_time this_year last_year this_month; do
  S0=$(date +%s%N)
  V=$(curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/stats?period=$p&limit=5")
  S1=$(date +%s%N)
  S2=$(date +%s%N); curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/stats?period=$p&limit=5" >/dev/null; S3=$(date +%s%N)
  echo "== $p ($(( (S1 - S0) / 1000000 )) ms; en caché $(( (S3 - S2) / 1000000 )) ms)"
  echo "$V" | jq -c '{unit: .period.unit, cubos: (.chart | length), totals: (.totals | {plays, horas: (.listening_ms / 3600000 | floor), sin_duracion: .plays_without_duration, artists, albums, tracks, avg_plays_per_day, top_day, change_pct})}'
  echo "$V" | jq -r '"  artistas: " + ([.top_artists[] | "\(.rank). \(.name) (\(.plays))"] | join(" · "))'
  echo "  canciones: $(echo "$V" | jq -r '[.top_tracks[] | "\(.rank). \(.title) — \(.artist) (\(.plays))"] | join(" · ")')"
  echo "  álbumes: $(echo "$V" | jq -r '[.top_albums[] | "\(.rank). \(.title) (\(.plays))"] | join(" · ")')"
done
grep -E "ERROR|WARN" "$WORK/ownfm.log" | sed 's/\x1b\[[0-9;]*m//g' | tail -5 || true
