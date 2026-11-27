#!/usr/bin/env bash
# Prueba de carga: importa un CSV sintético de N escuchas (3 años) y mide tiempos y memoria.
# Uso: scripts/e2e-carga.sh [N=100000] [binario-ownfm] [binario-navidrome]
set -euo pipefail

N=${1:-100000}
OWNFM_BIN=${2:-$(dirname "$0")/../server/target/x86_64-unknown-linux-musl/release/ownfm}
ND_BIN=${3:-$HOME/navidrome-pruebas/bin/navidrome}
WORK=$(mktemp -d)
ND_PORT=14535
OWNFM_PORT=18093
trap 'kill $(jobs -p) 2>/dev/null || true; rm -rf "$WORK"' EXIT

python3 - "$N" "$WORK/historial.csv" <<'EOF'
import random, sys, time
n, out = int(sys.argv[1]), sys.argv[2]
random.seed(7)
now = int(time.time())
artists = [f"Artista {i}" for i in range(150)]
albums = [(random.choice(artists), f"Álbum {i}") for i in range(300)]
tracks = [(al, f"Canción {i}") for i, al in enumerate(random.choices(albums, k=2000))]
weights = [1 / (i + 1) ** 0.8 for i in range(len(tracks))]  # unas pocas canciones se escuchan mucho
with open(out, "w") as f:
    f.write("played_at,artist,album,track,duration_ms,source\n")
    t = now - 3 * 365 * 86400
    step = (now - 3600 - t) // n
    for (art, alb), tit in random.choices(tracks, weights=weights, k=n):
        t += step
        f.write(f"{t},{art},{alb},{tit},{random.randint(120, 300) * 1000},import_file\n")
EOF
echo "CSV: $(wc -l < "$WORK/historial.csv") líneas, $(du -h "$WORK/historial.csv" | cut -f1)"

mkdir -p "$WORK/music" "$WORK/nd" "$WORK/ownfm"
ND_MUSICFOLDER="$WORK/music" ND_DATAFOLDER="$WORK/nd" ND_PORT=$ND_PORT ND_ADDRESS=127.0.0.1 ND_LOGLEVEL=warn "$ND_BIN" >"$WORK/nd.log" 2>&1 &
OWNFM_LISTEN=127.0.0.1:$OWNFM_PORT OWNFM_DATA_DIR="$WORK/ownfm" OWNFM_NAVIDROME_URL=http://127.0.0.1:$ND_PORT RUST_LOG=warn "$OWNFM_BIN" >"$WORK/ownfm.log" 2>&1 &
OWNFM_PID=$!
for _ in $(seq 60); do curl -sf "http://127.0.0.1:$ND_PORT/ping" >/dev/null && curl -sf "http://127.0.0.1:$OWNFM_PORT/api/v1/salud" >/dev/null && break; sleep 0.5; done
curl -sf -X POST "http://127.0.0.1:$ND_PORT/auth/createAdmin" -H 'Content-Type: application/json' -d '{"username":"pablo","password":"prueba123"}' >/dev/null
CJ="$WORK/c"
curl -sf -c "$CJ" -X POST "http://127.0.0.1:$OWNFM_PORT/api/v1/auth/login" -H 'X-Requested-With: e2e' -H 'Content-Type: application/json' \
  -d '{"username":"pablo","password":"prueba123"}' >/dev/null
echo "RSS en reposo: $(ps -o rss= -p $OWNFM_PID) KB"

T0=$(date +%s.%N)
JOB=$(curl -sf -b "$CJ" -X POST "http://127.0.0.1:$OWNFM_PORT/api/v1/import/file" -H 'X-Requested-With: e2e' -F "file=@$WORK/historial.csv" | jq .job.id)
MAXRSS=0
while :; do
  J=$(curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/import/jobs/$JOB")
  R=$(ps -o rss= -p $OWNFM_PID | tr -d ' '); [ "$R" -gt "$MAXRSS" ] && MAXRSS=$R
  case $(echo "$J" | jq -r .status) in done|failed) break;; esac
  sleep 1
done
T1=$(date +%s.%N)
printf 'Importación: %s en %.1f s\n' "$(echo "$J" | jq -c '{status, stats}')" "$(echo "$T1 - $T0" | bc)"
echo "RSS máximo importando: $MAXRSS KB · después: $(ps -o rss= -p $OWNFM_PID) KB · BD: $(du -sh "$WORK/ownfm" | cut -f1)"

echo "sqlite_stat1: $(sqlite3 "$WORK/ownfm/ownfm.db" "SELECT COUNT(*) FROM sqlite_stat1" 2>&1)"
for p in this_week this_month this_year last_year all_time; do
  S0=$(date +%s%N); V=$(curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/stats?period=$p"); S1=$(date +%s%N)
  curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/stats?period=$p" >/dev/null; S2=$(date +%s%N)
  printf '%-10s %6d escuchas · %4d ms (en caché %d ms) · top: %s\n' "$p" "$(echo "$V" | jq .totals.plays)" \
    $(( (S1 - S0) / 1000000 )) $(( (S2 - S1) / 1000000 )) "$(echo "$V" | jq -r '.top_tracks[0] | "\(.title) (\(.plays))"')"
done
S0=$(date +%s%N); curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/top/tracks?period=all_time&page=10&limit=50" >/dev/null; S1=$(date +%s%N)
echo "top/tracks página 10: $(( (S1 - S0) / 1000000 )) ms"
S0=$(date +%s%N); curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/export?format=csv" -o "$WORK/export.csv"; S1=$(date +%s%N)
echo "exportar CSV: $(( (S1 - S0) / 1000000 )) ms, $(wc -l < "$WORK/export.csv") líneas"
echo "RSS final: $(ps -o rss= -p $OWNFM_PID) KB"
if [ -n "${KEEP_DB:-}" ]; then sqlite3 "$WORK/ownfm/ownfm.db" ".backup '$KEEP_DB'" && echo "BD copiada en $KEEP_DB"; fi
grep -E "ERROR|WARN" "$WORK/ownfm.log" | sed 's/\x1b\[[0-9;]*m//g' | grep -v enriquecedor | tail -5 || true
