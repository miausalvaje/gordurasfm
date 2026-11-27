#!/usr/bin/env bash
# Entorno de demostración: Navidrome de pruebas con música inventada (con portadas) + OwnFM
# con dos años de historial. Se queda en marcha para revisarlo en el navegador.
#
# Uso: scripts/demo.sh [arrancar|parar] [binario-ownfm]
#   Web: http://<ip>:8090  ·  usuario «pablo», contraseña «demo1234» (cuenta de Navidrome de pruebas)
set -euo pipefail

ACCION=${1:-arrancar}
OWNFM_BIN=${2:-$(cd "$(dirname "$0")/.." && pwd)/server/target/debug/ownfm}
ND_BIN=${ND_BIN:-$HOME/navidrome-pruebas/bin/navidrome}
DEMO=${DEMO:-$HOME/demo}
ND_PORT=14540
OWNFM_PORT=${OWNFM_PORT:-8090}

parar() {
  for f in "$DEMO"/*.pid; do
    [ -f "$f" ] || continue
    kill "$(cat "$f")" 2>/dev/null || true
    rm -f "$f"
  done
}

if [ "$ACCION" = parar ]; then parar; echo "parado"; exit 0; fi
parar
mkdir -p "$DEMO/music" "$DEMO/nd" "$DEMO/ownfm"

if [ ! -f "$DEMO/catalogo.json" ]; then
  echo "== música de prueba (con portadas)"
  python3 - "$DEMO" <<'EOF'
import json, random, subprocess, sys, os
demo = sys.argv[1]
random.seed(42)
artistas = {
  "Rosalía": ["Motomami", "El Mal Querer"], "Bad Gyal": ["Slow Wine", "La Joia"], "Los Planetas": ["Una semana en el motor de un autobús"],
  "Vetusta Morla": ["Mapas", "Mismo sitio, distinto lugar"], "C. Tangana": ["El Madrileño"], "Zahara": ["Puta"],
  "Carolina Durante": ["Cuatro chavales"], "Nathy Peluso": ["Calambre"], "Love of Lesbian": ["1999"], "Triángulo de Amor Bizarro": ["Salve discordia"],
  "Amaia": ["Pero no pasa nada"], "Hinds": ["Leave Me Alone"], "Sílvia Pérez Cruz": ["Granada"], "Mujeres": ["Siento muerte"],
}
palabras = "noche fuego mar ciudad luz frío verano ruido calle cielo sombra lluvia tiempo polvo corazón amigo final perro tigre azul".split()
cat = []
for i, (art, albums) in enumerate(artistas.items()):
    for j, alb in enumerate(albums):
        h1, h2 = random.randint(0, 359), random.randint(0, 359)
        cover = f"{demo}/cover-{i}-{j}.png"
        c1 = "0x%02x%02x%02x" % tuple(random.randint(30, 230) for _ in range(3))
        c2 = "0x%02x%02x%02x" % tuple(random.randint(10, 150) for _ in range(3))
        subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", f"gradients=s=500x500:c0={c1}:c1={c2}:n=2:type=radial:seed={i*10+j}", "-frames:v", "1", cover], check=True)
        for k in range(random.randint(6, 11)):
            title = " ".join(random.sample(palabras, random.randint(1, 3))).capitalize()
            dur = random.randint(140, 290)
            path = f"{demo}/music/{i:02d}-{j}-{k:02d}.mp3"
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", f"sine=frequency={200+k*40}:duration=4", "-i", cover,
                            "-map", "0:a", "-map", "1:v", "-c:a", "libmp3lame", "-b:a", "48k", "-c:v", "png", "-disposition:v", "attached_pic",
                            "-id3v2_version", "3", "-metadata", f"title={title}", "-metadata", f"artist={art}", "-metadata", f"album={alb}",
                            "-metadata", f"album_artist={art}", "-metadata", f"track={k+1}", path], check=True)
            cat.append({"artist": art, "album": alb, "track": title, "duration_ms": dur * 1000})
json.dump(cat, open(f"{demo}/catalogo.json", "w"), ensure_ascii=False)
print(len(cat), "canciones")
EOF
fi

echo "== Navidrome de pruebas (127.0.0.1:$ND_PORT)"
ND_MUSICFOLDER="$DEMO/music" ND_DATAFOLDER="$DEMO/nd" ND_PORT=$ND_PORT ND_ADDRESS=127.0.0.1 ND_LOGLEVEL=warn \
  ND_LISTENBRAINZ_ENABLED=true ND_LISTENBRAINZ_BASEURL="http://127.0.0.1:$OWNFM_PORT/1/" \
  nohup "$ND_BIN" >"$DEMO/nd.log" 2>&1 & echo $! >"$DEMO/nd.pid"

echo "== OwnFM (0.0.0.0:$OWNFM_PORT)"
OWNFM_LISTEN=0.0.0.0:$OWNFM_PORT OWNFM_DATA_DIR="$DEMO/ownfm" OWNFM_NAVIDROME_URL=http://127.0.0.1:$ND_PORT \
  OWNFM_PUBLIC_URL="http://$(hostname -I | awk '{print $1}'):$OWNFM_PORT" RUST_LOG=info \
  nohup "$OWNFM_BIN" >"$DEMO/ownfm.log" 2>&1 & echo $! >"$DEMO/ownfm.pid"

for _ in $(seq 60); do curl -sf "http://127.0.0.1:$ND_PORT/ping" >/dev/null && curl -sf "http://127.0.0.1:$OWNFM_PORT/api/v1/salud" >/dev/null && break; sleep 0.5; done

if [ ! -f "$DEMO/listo" ]; then
  echo "== cuenta de Navidrome y escaneo"
  curl -sf -X POST "http://127.0.0.1:$ND_PORT/auth/createAdmin" -H 'Content-Type: application/json' -d '{"username":"pablo","password":"demo1234"}' >/dev/null
  SUB="u=pablo&p=demo1234&v=1.16.1&c=demo&f=json"
  curl -sf "http://127.0.0.1:$ND_PORT/rest/startScan?$SUB" >/dev/null
  N=$(python3 -c "import json;print(len(json.load(open('$DEMO/catalogo.json'))))")
  for _ in $(seq 120); do
    c=$(curl -sf "http://127.0.0.1:$ND_PORT/rest/getScanStatus?$SUB" | jq '.["subsonic-response"].scanStatus.count')
    [ "$c" = "$N" ] && break; sleep 1
  done
  echo "   $c canciones en Navidrome"

  echo "== historial inventado (2 años) e importación"
  python3 - "$DEMO" <<'EOF'
import json, random, sys, time, csv
demo = sys.argv[1]
random.seed(7)
cat = json.load(open(f"{demo}/catalogo.json"))
# Cada artista tiene su época; unas pocas canciones se escuchan mucho.
weights = [1 / (i + 1) ** 0.9 for i in range(len(cat))]
random.shuffle(weights)
now = int(time.time())
t = now - 2 * 365 * 86400
rows = []
while t < now - 600:
    # Más por la tarde y noche, menos de madrugada.
    lt = time.localtime(t)
    rate = [0.1,0.05,0.02,0.02,0.02,0.05,0.2,0.5,0.8,0.7,0.6,0.7,0.9,1,0.8,0.8,1,1.2,1.5,1.6,1.5,1.2,0.8,0.4][lt.tm_hour]
    if random.random() < rate * 0.5:
        s = random.choices(cat, weights=weights)[0]
        rows.append((t, s["artist"], s["album"], s["track"], s["duration_ms"]))
        t += s["duration_ms"] // 1000 + random.randint(0, 20)
    else:
        t += random.randint(600, 3600)
with open(f"{demo}/historial.csv", "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["played_at", "artist", "album", "track", "duration_ms", "source"])
    for r in rows:
        w.writerow([*r, "import_file"])
print(len(rows), "escuchas")
EOF
  CJ="$DEMO/cookies"
  curl -sf -c "$CJ" -X POST "http://127.0.0.1:$OWNFM_PORT/api/v1/auth/login" -H 'X-Requested-With: demo' -H 'Content-Type: application/json' \
    -d '{"username":"pablo","password":"demo1234"}' >/dev/null
  JOB=$(curl -sf -b "$CJ" -X POST "http://127.0.0.1:$OWNFM_PORT/api/v1/import/file" -H 'X-Requested-With: demo' -F "file=@$DEMO/historial.csv" | jq .job.id)
  for _ in $(seq 300); do
    S=$(curl -sf -b "$CJ" "http://127.0.0.1:$OWNFM_PORT/api/v1/import/jobs/$JOB" | jq -r .status)
    [ "$S" = done ] || [ "$S" = failed ] && break; sleep 1
  done
  echo "   importación: $S"
  touch "$DEMO/listo"
fi
echo "Listo: http://$(hostname -I | awk '{print $1}'):$OWNFM_PORT  (pablo / demo1234)"
