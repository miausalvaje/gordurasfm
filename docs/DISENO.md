# GordurasFM: diseño (fase 0)

Plataforma autoalojada de estadísticas de escucha (estilo Last.fm + Wrapped) para los usuarios de un Navidrome.

Leyenda: ✅ comprobado · 💡 propuesta · ⚠️ limitación o riesgo · ❓ pendiente de decidir

---

## 1. Análisis de requisitos

### Lo que hay que construir
| Bloque | Resumen |
|---|---|
| Ingesta | Endpoint compatible con ListenBrainz para Navidrome, sondeo de `getNowPlaying` como respaldo, importación desde Last.fm (API, CSV o JSON) |
| Reenvío | Cola persistente hacia ListenBrainz y Last.fm, con reintentos y destinos que cada usuario activa o desactiva |
| Identidad | Login con las credenciales de Navidrome (Subsonic). Se guardan token y salt cifrados y nunca la contraseña |
| Estadísticas | 7 periodos, gráfica por cubos, totales, Top Albums, Top Tracks y artistas, con la zona horaria de cada usuario |
| Wrapped | Fotos fijas anuales en JSON, Stories de 15 diapositivas, 3 pruebas, música de Navidrome, exportación a imagen |
| Entrega | Un solo binario estático (API + web embebida), systemd en LXC, Docker multiarquitectura y Termux |

### Datos comprobados que condicionan el diseño
- ✅ **Navidrome de producción: 0.64.2** (LXC 105). Es también la última versión publicada (GitHub, 2026-09-24).
- ✅ El agente ListenBrainz de Navidrome 0.64.2 (`adapters/listenbrainz/`):
  - Llama a `BaseURL + validate-token` (GET) y a `BaseURL + submit-listens` (POST) con la cabecera `Authorization: Token <token>`.
  - Envía `listen_type` `playing_now` (sin `listened_at`) y `single` (con `listened_at` = inicio de la reproducción).
  - Manda `artist_name`, `track_name`, `release_name`, `duration_ms`, `tracknumber`, `artist_names` y los MBID (grabación, release, release group y artistas) si las etiquetas los tienen.
  - **No manda el id de la canción en Navidrome**, así que portadas e ids hay que resolverlos aparte (ver §3.4).
  - Si la respuesta es un error de red, 500 o 503, Navidrome **guarda la escucha y la reintenta** (`ErrRetryLater`). Cualquier otro error la descarta.
  - ⚠️ **Usa la misma `BaseURL` para metadatos** (`metadata/artist`, `popularity/top-recordings-for-artist/…`). Si apuntamos la `BaseURL` a GordurasFM, esas peticiones nos llegan a nosotros, así que hay que reenviarlas tal cual a ListenBrainz (ver §3.2).
- ⚠️ `ND_LISTENBRAINZ_BASEURL` afecta **a toda la instancia**: todos los usuarios de ese Navidrome pasan por GordurasFM.

### Decisiones (confirmadas el 2026-11-25)
1. ✅ Interfaz **en español**, con todos los textos en un solo archivo (`lib/l10n/es.dart`) para traducirla más tarde.
2. ✅ **Key de Last.fm de la instancia** (la configura el admin) y, opcionalmente, una propia por usuario.
3. ✅ Entra **cualquier usuario del Navidrome**; el primero es admin. Interruptor `registration = "approval"` para exigir aprobación.
4. ✅ Repositorio **público `miausalvaje/gordurasfm`, AGPL-3.0**.
5. ✅ Entorno de desarrollo: **LXC 135 `gordurasfm-dev`** (§10).
6. ✅ Se reconocen los tokens reales de ListenBrainz ya conectados y los desconocidos pasan tal cual a ListenBrainz (§3.2).

---

## 2. Arquitectura

```
                 ┌──────────────── Navidrome 0.64.2 (LXC 105) ───────────────┐
                 │  ND_LISTENBRAINZ_BASEURL=http://gordurasfm:8090/1/        │
                 │  cada usuario pega su token de GordurasFM en su perfil    │
                 └───────┬──────────────────────────────▲────────────────────┘
      submit-listens /   │                              │ API Subsonic (login, búsqueda,
      validate-token     │                              │ portadas, stream, getNowPlaying)
                         ▼                              │
┌──────────────────────── gordurasfm (un binario Rust) ─┴─────────────────────────┐
│ axum                                                                            │
│  /1/*            compatibilidad ListenBrainz (+ paso a LB para metadatos)       │
│  /api/v1/*       API REST propia (OpenAPI en /api/docs)                         │
│  /api/v1/media/* proxy de portadas, imágenes de artista y audio                 │
│  /*              Flutter Web embebido (rust-embed)                              │
│                                                                                 │
│ Tareas de fondo (tokio):                                                        │
│  forwarder   cola → ListenBrainz / Last.fm, reintentos con espera creciente     │
│  importer    Last.fm por páginas, reanudable; CSV/JSON                          │
│  enricher    empareja escuchas con ids de Navidrome, guarda portadas            │
│  poller      getNowPlaying (respaldo, desactivado por defecto)                  │
│  wrapped     genera fotos fijas anuales; copias de la BD cada noche             │
│                                                                                 │
│ SQLite WAL  /var/lib/gordurasfm/gordurasfm.db   + images/ (portadas en caché)  │
└──────────────────────┬───────────────────────────────┬─────────────────────────┘
                       ▼                               ▼
            api.listenbrainz.org                 ws.audioscrobbler.com
```

### Backend (Rust)
- Un solo crate `server/`, con módulos `config`, `db`, `crypto`, `auth`, `subsonic`, `listenbrainz` (compatibilidad y cliente), `lastfm`, `scrobble` (ingesta, normalización y duplicados), `forward`, `stats`, `wrapped`, `media`, `jobs`, `api` y `web`.
- Crates: `axum`, `tokio`, `sqlx` (sqlite, migrate, SQLite empaquetado), `reqwest` (rustls + `webpki-roots` + `hickory-dns`, sin OpenSSL), `tower-http` (compresión, límites, trazas), `chrono` + `chrono-tz` (zonas horarias incluidas en el binario), `chacha20poly1305`, `sha2`, `md-5`, `unicode-normalization`, `rust-embed`, `utoipa` + `utoipa-scalar`, `moka` (caché), `tracing`.
- **Binario estático**: `x86_64-unknown-linux-musl` y `aarch64-unknown-linux-musl` con `cargo-zigbuild`. No depende de glibc, OpenSSL, zoneinfo ni certificados del sistema, así que el mismo binario sirve para Ubuntu, Docker `scratch` y Termux.
- Configuración: `config.toml`, que se puede sobrescribir con variables `GFM_*`. Los secretos de la instancia (clave maestra y key de Last.fm) van en archivos con permisos 600, nunca en la BD en claro.
- Consumo: 💡 estimado entre 15 y 40 MB de RAM en reposo y CPU casi nula. Se medirá en la fase 5.

### Frontend (Flutter Web)
- `web/` con `go_router`, `flutter_riverpod` (estado), `fl_chart`, `flutter_animate`, `just_audio` y `http`.
- Rutas: `/login`, `/` (estadísticas), `/top/albums`, `/top/tracks`, `/top/artists`, `/historial`, `/perfil`, `/u/:usuario` (perfil público), `/wrapped`, `/wrapped/:año`, `/ajustes/conexiones`, `/admin`.
- Se compila con `flutter build web --release` y el binario lo embebe: la API y la web salen del mismo origen, sin CORS y con cookie de sesión `HttpOnly`.
- ⚠️ Flutter Web pesa: la primera carga ronda los 2-3 MB (estimado). Después queda en la caché del navegador. Para una app de uso diario en la red de casa es aceptable.

---

## 3. Navidrome, proxy de ListenBrainz y Last.fm

### 3.1 Login
1. El usuario escribe su usuario y su contraseña de Navidrome en GordurasFM.
2. El backend genera un `salt` aleatorio, calcula `token = md5(contraseña + salt)` y llama a `ping` y `getUser` de Subsonic con `u`, `t` y `s`.
3. Si funciona, crea o actualiza el usuario y guarda `(token, salt)` **cifrados** con XChaCha20-Poly1305 y la clave maestra. La contraseña se descarta en ese momento.
4. Emite una sesión propia: 256 bits aleatorios, en BD solo su SHA-256, en una cookie `gfm_session` `HttpOnly`, `SameSite=Lax` y `Secure` si va por HTTPS. Dura 30 días y se renueva con el uso.
5. ⚠️ Si el usuario cambia la contraseña en Navidrome, el token guardado deja de valer. GordurasFM lo detecta en la siguiente llamada, marca «reconectar» y pide entrar otra vez. Las escuchas **siguen llegando**, porque usan otro token.
6. ⚠️ Ojo con `md5(contraseña+salt)`: es un hash débil de la contraseña. Por eso va cifrado y la clave maestra está en un archivo aparte (600).

### 3.2 Endpoint compatible con ListenBrainz
| Ruta | Comportamiento |
|---|---|
| `GET /1/validate-token` | `{"code":200,"message":"Token valid.","valid":true,"user_name":"<usuario>"}`. Si el token no vale, `valid:false` |
| `POST /1/submit-listens` | Acepta `single`, `import` y `playing_now`. Valida como ListenBrainz: `listened_at` obligatorio salvo en `playing_now` y un máximo de 1000 escuchas por petición. Contesta `{"status":"ok"}` |
| `GET /1/<resto>` | **Paso transparente** a `https://api.listenbrainz.org/1/<resto>`, con caché corta, para que la información de artista de Navidrome siga funcionando |

Al llegar una escucha `single`:
1. Se busca al usuario por su token.
2. Se normaliza y se comprueba si está duplicada (§4).
3. Se guarda en `scrobbles` y en `forward_queue` (una fila por destino activo), **en la misma transacción**.
4. Se contesta 200 enseguida. El reenvío lo hace el `forwarder` por detrás.
5. Si no se puede escribir en la BD (disco lleno, BD bloqueada), se contesta **503**. Así Navidrome guarda la escucha y la reintenta, y no se pierde nada.

Las `playing_now` no se guardan como escucha: van a la memoria («escuchando ahora» del perfil) y se reenvían al momento, sin cola, porque no tiene sentido reintentar algo que ya ha pasado.

💡 **Tokens de ListenBrainz ya enlazados.** Si un usuario de Navidrome ya tenía puesto su token real de ListenBrainz, al cambiar la `BaseURL` ese token llegaría a GordurasFM y se rechazaría. Propuesta:
- Si el token coincide con el token de ListenBrainz que un usuario conectó en GordurasFM (búsqueda por HMAC, nunca en claro), se trata como suyo. **Así no tiene que cambiar nada en Navidrome.**
- Si no coincide con nadie, la petición pasa tal cual a ListenBrainz y no se guarda. Los usuarios que no usan GordurasFM no notan nada.

### 3.3 Reenvío (`forwarder`)
- Tabla `forward_queue(scrobble_id, destino, estado, intentos, siguiente_intento, último_error)`.
- Agrupa por usuario y destino: hasta 100 por petición a ListenBrainz (`import`) y 50 a Last.fm (`track.scrobble`).
- Reintentos con espera creciente y algo de azar: 30 s, 1 min, 2 min… hasta 6 h. Respeta `Retry-After` y `X-RateLimit-*`.
- Errores permanentes (token revocado, 401) → estado `failed`, y aviso en el perfil para reconectar. Al reconectar se reintentan.
- ⚠️ **Last.fm rechaza escuchas de hace más de unas dos semanas** (código *Timestamp too old*). Si Last.fm pasa caído más tiempo, esas se marcan `dead` y no se pueden recuperar. ListenBrainz no tiene ese límite.
- Para evitar bucles, **solo se reenvían escuchas con origen `navidrome` o `navidrome_poll`**. Las importadas de Last.fm nunca vuelven a Last.fm ni van a ListenBrainz (allí estarían duplicadas). Si alguien quiere copiar su historial de Last.fm a ListenBrainz, ListenBrainz ya tiene su propio importador.
- Cada usuario activa o desactiva cada destino. Al desactivarlo, lo pendiente se queda en espera (no se borra).

### 3.4 Datos de Navidrome: ids, portadas e imágenes (`enricher`)
- Como la escucha no trae el id de Navidrome, el `enricher` busca la canción con `search3` usando las credenciales **del propio usuario** (respeta sus bibliotecas y permisos). Empareja por MBID de grabación si lo hay y, si no, por artista + título + álbum normalizados.
- Guarda en `tracks`, `albums` y `artists` el id de Navidrome y el `coverArt`.
- Proxy de medios (cookie de sesión obligatoria, nunca se manda nada de Subsonic al navegador):
  - `GET /api/v1/media/cover/:ref?size=300` → `getCoverArt`, que se guarda en `images/` (por hash de contenido) para cargas rápidas y para el Wrapped.
  - `GET /api/v1/media/artist/:id` → `getArtistInfo2` / `getCoverArt(ar-…)`. Si no hay imagen, la API devuelve hasta 4 portadas (`covers`) y el collage lo hace la web (cambio de la fase 2: así el binario no necesita una librería de imágenes).
  - `GET /api/v1/media/stream/:song_id` → `stream` con `format=opus&maxBitRate=128`, admitiendo cabecera `Range`, para la música del Wrapped.
- Canciones importadas de Last.fm que no están en Navidrome: se usa la portada de Last.fm (URL de la importación), descargada solo para lo que sale en tops y Wrapped.

### 3.5 Respaldo con `getNowPlaying` (`poller`)
- Está desactivado por defecto. Lo activa el admin indicando una cuenta de Navidrome cuyo token se usa para sondear.
- Cada 10 s llama a `getNowPlaying` y sigue cada sesión `(usuario, reproductor, canción)`.
- Cuando la canción cambia o desaparece y el tiempo observado es ≥ min(50 % de la duración, 4 min), con canciones de ≥ 30 s, registra la escucha con `source = navidrome_poll` y `played_at` = cuando se vio por primera vez.
- ⚠️ La precisión es de unos ±10 s y `getNowPlaying` no sabe de pausas, así que una pausa larga puede contar como escucha.
- Para no contar dos veces, **solo se aplica a usuarios sin escuchas por el endpoint en los últimos 7 días**. Además se pasa siempre el filtro de duplicados de ±60 s.

### 3.6 Last.fm: conexión
- Flujo web de Last.fm: `https://www.last.fm/api/auth/?api_key=…&cb=<GordurasFM>/api/v1/lastfm/callback`, después `auth.getSession` firmado con el *secret* y se guarda la *session key* cifrada. No caduca.
- Key de la instancia por defecto; el usuario puede poner la suya (key + secret cifrado) en «Conexiones».
- ⚠️ La URL de vuelta tiene que ser accesible desde el navegador del usuario (en la red de casa, vale la IP local).

---

## 4. Importación y escuchas

### 4.1 Importar de Last.fm (`importer`)
- Trabajo en `jobs`, con el cursor guardado: `{to: <instante fijo al empezar>, page: N, total_pages}`. Si el servicio se reinicia, sigue por la última página buena.
- `user.getRecentTracks` con `limit=200&extended=1&to=…`, de lo más nuevo a lo más viejo. Se ignora la fila `nowplaying`.
- Límite: como mucho 4 peticiones/s; ante 429/500/29 espera creciente. Progreso = página / total, con barra en la web.
- 💡 Tiempo estimado: 100 000 escuchas → 500 páginas → entre 2 y 10 min según cómo responda Last.fm.
- **Resincronizar**: `from = última escucha importada − 1 h` y solo trae lo nuevo. El filtro de duplicados quita el solape.
- Por fichero (sin API): CSV `artist,album,track,date` (el de herramientas como *lastfm-to-csv*), JSON de exportación de ListenBrainz y JSON propio de GordurasFM. Se sube por la web y se procesa con el mismo trabajo.
- Las escuchas importadas llevan `source = lastfm` (o `import_file`).

### 4.2 Normalización
- Clave de comparación = NFKC + minúsculas Unicode + espacios recortados y colapsados. No se quitan «feat.» ni paréntesis, igual que Last.fm.
- Se muestra el nombre de la primera vez que aparece; el admin puede fusionar artistas o álbumes duplicados.
- Una **canción** es (artista, título), como en Last.fm: la misma canción en dos álbumes suma en el mismo Top Track. El álbum va en cada escucha, y Top Albums agrupa por álbum.
- Un **álbum** es (artista del álbum, título). Si no hay artista del álbum, se usa el de la pista hasta que el `enricher` lo corrija con Navidrome (recopilatorios).

### 4.3 Duplicados
Una escucha es duplicada si el mismo usuario ya tiene **la misma canción a ±60 s**. Se aplica a todos los orígenes, así que importar dos veces o mezclar Last.fm con Navidrome no duplica. Además se descartan pistas de menos de 30 s, como en Last.fm. La regla de 50 % / 4 min ya la aplica Navidrome (o el `poller`).

### 4.4 Qué se guarda por escucha
Usuario, canción, artista, álbum, portada (por referencia al álbum), duración, `played_at` (segundos Unix en UTC), origen, cliente y el **JSON original** (`raw`). El JSON original permite volver a normalizar si cambian las reglas. El número total de reproducciones se calcula con `COUNT` sobre índices y no se guarda (no se puede desincronizar).

---

## 5. Base de datos (SQLite, WAL)

`PRAGMA journal_mode=WAL; synchronous=NORMAL; foreign_keys=ON; busy_timeout=5000`. Las migraciones van numeradas en `server/migrations/` (sqlx) y se aplican al arrancar.

```sql
users(id INTEGER PK, username TEXT UNIQUE NOT NULL,     -- usuario de Navidrome
      display_name TEXT, is_admin INT, is_approved INT,
      timezone TEXT NOT NULL DEFAULT 'Europe/Madrid',
      is_public INT DEFAULT 0, created_at INT, last_login_at INT)

navidrome_credentials(user_id PK→users, secret_enc BLOB,  -- (token,salt) cifrados
      status TEXT, updated_at INT)

sessions(id INTEGER PK, user_id→users, token_hash BLOB UNIQUE,
      created_at INT, expires_at INT, last_seen_at INT, user_agent TEXT)

api_tokens(id INTEGER PK, user_id→users, kind TEXT,        -- 'scrobble' | 'api'
      token_hash BLOB UNIQUE, label TEXT, created_at INT, last_used_at INT)

connections(user_id→users, provider TEXT,                  -- 'listenbrainz' | 'lastfm'
      remote_username TEXT, secret_enc BLOB, lookup_hmac BLOB,
      own_api_key TEXT, own_api_secret_enc BLOB,
      forward_enabled INT, status TEXT, last_error TEXT, updated_at INT,
      PRIMARY KEY(user_id, provider))

artists(id INTEGER PK, name TEXT, name_key TEXT UNIQUE, mbid TEXT,
      navidrome_id TEXT, image_ref TEXT)
albums(id INTEGER PK, title TEXT, title_key TEXT, artist_id→artists, mbid TEXT,
      navidrome_id TEXT, cover_ref TEXT, UNIQUE(artist_id, title_key))
tracks(id INTEGER PK, title TEXT, title_key TEXT, artist_id→artists,
      duration_ms INT, mbid TEXT, UNIQUE(artist_id, title_key))
track_navidrome(track_id→tracks, album_id→albums, navidrome_song_id TEXT,
      PRIMARY KEY(track_id, album_id))

scrobbles(id INTEGER PK, user_id→users, track_id→tracks, album_id→albums NULL,
      artist_id→artists, played_at INT NOT NULL, duration_ms INT,
      source TEXT NOT NULL, client TEXT, raw TEXT, created_at INT)
  INDEX(user_id, played_at)            -- periodos y gráficas
  INDEX(user_id, track_id, played_at)  -- duplicados y top tracks
  INDEX(user_id, album_id, played_at)
  INDEX(user_id, artist_id, played_at)

forward_queue(id INTEGER PK, scrobble_id→scrobbles, user_id, target TEXT,
      status TEXT, attempts INT, next_attempt_at INT, last_error TEXT,
      updated_at INT, UNIQUE(scrobble_id, target))
  INDEX(status, next_attempt_at)

jobs(id INTEGER PK, user_id, kind TEXT, status TEXT, cursor TEXT,
      progress_done INT, progress_total INT, stats TEXT, error TEXT,
      created_at INT, updated_at INT)

wrapped(id INTEGER PK, user_id→users, year INT, version INT, is_final INT,
      generated_at INT, data TEXT NOT NULL, UNIQUE(user_id, year, version))
wrapped_answers(wrapped_id→wrapped, question TEXT, chosen_rank INT,
      correct INT, answered_at INT, PRIMARY KEY(wrapped_id, question))

images(hash TEXT PK, mime TEXT, width INT, height INT, source TEXT, created_at INT)
settings(key TEXT PK, value TEXT)      -- ajustes de la instancia editables por el admin
```

Cada consulta de datos lleva `WHERE user_id = <sesión>`. No hay forma de pedir el `user_id` de otro, salvo por `/u/:usuario` si ese perfil es público, y solo en lectura.

---

## 6. Cálculo de estadísticas

### Periodos (con la zona horaria del usuario, por defecto `Europe/Madrid`)
| Periodo | Desde | Hasta | Cubos de la gráfica |
|---|---|---|---|
| Esta semana | lunes 00:00 | ahora | 7 días |
| Semana pasada | lunes anterior 00:00 | lunes 00:00 | 7 días |
| Este mes | día 1 00:00 | ahora | días del mes |
| Mes pasado | día 1 del mes anterior | día 1 de este mes | días |
| Este año | 1 ene 00:00 | ahora | 12 meses |
| Año pasado | 1 ene del año anterior | 1 ene de este año | 12 meses |
| Siempre | primera escucha | ahora | meses si son ≤ 36; si no, años |

- **Los límites se calculan en hora local y luego se pasan a UTC** con `chrono-tz`. Así un día de 23 o 25 h (cambio de hora) es un solo cubo correcto. Habrá tests con el 29-03-2026 y el 25-10-2026 en Madrid.
- Gráfica: Rust genera la lista de cubos `[inicio, fin)` en UTC y una sola consulta cuenta por rango:
  ```sql
  WITH b(i, s, e) AS (VALUES (0, ?, ?), (1, ?, ?), …)
  SELECT b.i, COUNT(x.id) FROM b
  LEFT JOIN scrobbles x ON x.user_id = ? AND x.played_at >= b.s AND x.played_at < b.e
  GROUP BY b.i ORDER BY b.i;
  ```
- Totales del periodo: escuchas, **tiempo escuchado** (suma de `duration_ms` de la escucha o, si falta, de la canción; las que no tienen duración se cuentan aparte y el total sale como «≈» con el número de escuchas sin duración), artistas, álbumes y canciones distintos, media diaria, día con más escuchas y comparación con el periodo anterior (%).
- Tops (`LIMIT n`, paginables):
  ```sql
  SELECT album_id, COUNT(*) AS plays, MAX(played_at) AS last
  FROM scrobbles WHERE user_id = ? AND played_at >= ? AND played_at < ? AND album_id IS NOT NULL
  GROUP BY album_id ORDER BY plays DESC, last DESC LIMIT ?;
  ```
  Las posiciones empatadas comparten número (1, 2, 2, 4).
- Caché en memoria (`moka`) por `(usuario, periodo, zona, versión_de_datos)`. La versión sube con cada escucha o importación, así que nunca se ven datos viejos.
- Endpoint principal: `GET /api/v1/stats?period=this_week` devuelve gráfica, totales y los 3 tops de una vez (lo que necesita la portada). Los tops completos van en `/api/v1/top/{albums|tracks|artists}?period=…&page=…`.

---

## 7. Wrapped

### 7.1 Generación y foto fija
- `wrapped.data` guarda un JSON **completo y desnormalizado**: nombres, posiciones, recuentos, referencias de imagen a `images/` (copias propias, no de Navidrome) e ids de las canciones de fondo. Si la biblioteca cambia o se borra un álbum, el Wrapped sigue igual.
- **Vista previa** durante el año: se genera al pedirla, se guarda como `is_final = 0` y se rehace como mucho una vez por hora.
- **Definitivo**: una tarea revisa cada hora a los usuarios para los que ya es 1 de enero a las 06:00 en su zona y genera `version = 1, is_final = 1` del año anterior. Se espera hasta las 06:00 para que lleguen las escuchas que Navidrome tuviera guardadas.
- Si después se importan escuchas de ese año (por ejemplo, desde Last.fm), el perfil ofrece «Rehacer Wrapped 2025». Se crea una `version 2` y la 1 se conserva.
- Con el historial de Last.fm importado se pueden generar los Wrapped de años anteriores al momento («Generar Wrapped de 2019…»).

### 7.2 Contenido del JSON (esquema resumido)
```json
{
  "schema": 1, "year": 2026, "user": "pablo", "timezone": "Europe/Madrid",
  "totals": {"plays": 18234, "ms": 3820000000, "ms_estimated": false,
             "artists": 812, "albums": 1430, "tracks": 5210},
  "top_artists": [{"rank":1,"name":"…","plays":904,"image":"img:ab12…"}],
  "top_albums":  [{"rank":1,"title":"…","artist":"…","plays":311,"cover":"img:…"}],
  "top_tracks":  [{"rank":1,"title":"…","artist":"…","album":"…","plays":97,"cover":"img:…"}],
  "monthly": [{"month":1,"plays":1203,"top_artist":"…"}],
  "evolution": [{"quarter":1,"top_artists":["…","…","…"]}],
  "top_month": {"month":7,"plays":2310},
  "quiz": {"artist":[1,2,3,4,5], "album":[…], "track":[…]},
  "soundtrack": [{"navidrome_song_id":"…","title":"…","artist":"…"}],
  "generated_at": 1798761600
}
```
(Las cifras del ejemplo son de mentira, solo enseñan la forma.)

### 7.3 Stories (Flutter)
- Un `WrappedPlayer` con las 15 diapositivas (§8 de tus requisitos) en un `PageView` sin gestos libres, controlado por un temporizador por diapositiva (5-8 s), con barras de progreso arriba. Tocar a la derecha avanza, a la izquierda retrocede y mantener pulsado pausa.
- Formato 9:16: en móvil a pantalla completa; en escritorio, un lienzo 9:16 centrado sobre un fondo desenfocado de la portada principal.
- Animaciones con `flutter_animate` y fondos con degradados animados dibujados con `CustomPainter` (formas que se mueven). La paleta se saca de las portadas. Diseño propio, sin recursos ni identidad de Spotify.
- **Pruebas** (diapositivas 4, 6 y 8): el temporizador se para; aparecen las opciones (top 5 real, o las que haya si hay menos), mezcladas con `Random` en cada visualización. Se elige una, se pulsa «Confirmar», hay 1,5 s de suspense (redoble visual y oscurecido) y se revela con una animación de acierto o fallo y el ranking del 1 al 5 con las escuchas exactas. La respuesta se guarda en `wrapped_answers`. La diapositiva 15 enseña «¡Has acertado X de 3!».
- Si un año tiene menos de 5 artistas, álbumes o canciones distintos, la prueba enseña los que haya; si solo hay 1, se salta la prueba y se pasa directamente a la revelación.

### 7.4 Música de fondo
- Pantalla inicial con el botón «Empezar»: el toque desbloquea el audio del navegador.
- Un único `AudioPlayer` de `just_audio` vive **por encima** del `PageView`, así que cambiar de diapositiva no lo reinicia. Lista con las canciones de `soundtrack` (las más escuchadas del año que existen en Navidrome), por `/api/v1/media/stream/:id`. Botón de silencio siempre visible y fundido de volumen al cambiar de canción.
- Si una canción ya no está en Navidrome, se salta. Si no queda ninguna, el Wrapped funciona sin música.

### 7.4 bis Cómo quedó al programarlo (fase 4) ✅
- Sin `just_audio`: un `HTMLAudioElement` del navegador (`web/lib/platform/music_web.dart`) que vive en el reproductor, por encima de las diapositivas. Salta las canciones que fallan y hace fundidos de volumen.
- Sin `PageView` ni `flutter_animate` en las diapositivas: un `AnimatedSwitcher` y animaciones propias (`Reveal`, `CountUp`) movidas por un único controlador de entrada. Así cada diapositiva se puede pintar «quieta» (estado final) para exportarla.
- Lienzo lógico de 360 × 640 escalado con `FittedBox`. En el móvil se alarga hasta llenar la pantalla; al exportar se fuerza 9:16 y se captura a ×3 (1080 × 1920).
- Paleta: se reducen a 20 × 20 la portada y la foto del artista principales y se eligen los colores más vivos y distintos entre sí.
- «Exportar todo»: ZIP sin compresión hecho en Dart (los PNG ya van comprimidos), sin dependencias.
- Las respuestas se guardan con `POST /api/v1/wrapped/answers/{id}`; el servidor decide si es acierto (posición 1, con empates incluidos).

### 7.5 Compartir y exportar
- Cada diapositiva está dentro de un `RepaintBoundary` → `toImage()` → PNG de 1080×1920.
- «Compartir» usa la Web Share API con archivos si el navegador la tiene (Android Chrome, Safari iOS); si no, descarga. «Exportar todo» descarga un ZIP con las 15 imágenes (las pruebas, ya contestadas).

---

## 8. Despliegue

### 8.1 LXC Ubuntu 24.04 en Proxmox (principal, sin Docker)
- `deploy/lxc/install.sh`:
  1. Detecta la arquitectura.
  2. Descarga el binario de la *release* de GitHub (o usa uno local con `--binary`) y comprueba su SHA-256.
  3. Crea el usuario de sistema `gordurasfm`, `/var/lib/gordurasfm` (BD, imágenes y copias) y `/etc/gordurasfm/config.toml`.
  4. Genera la clave maestra en `/etc/gordurasfm/master.key` (600) e instala la unidad systemd.
  5. Instala `gordurasfm update` para las actualizaciones (copia la BD antes de cambiar el binario).
- La unidad systemd lleva `ProtectSystem=strict`, `ReadWritePaths=/var/lib/gordurasfm`, `NoNewPrivileges`, `PrivateTmp` y `MemoryMax=256M`. Funciona en un LXC sin privilegios.
- Recursos propuestos: 1 vCPU, 512 MB de RAM y 4 GB de disco.
- Navidrome (LXC 105): añadir `ListenBrainz.BaseURL = "http://<IP-gordurasfm>:8090/1/"` en `navidrome.toml` y reiniciar. ⚠️ Es un cambio en producción y se hará **solo con tu permiso**, en la fase 1, después de probarlo con un Navidrome de pruebas.
- Acceso desde fuera por NPMplus (`gordurasfm.miausalvaje.es`), si lo quieres: con la skill de abrir dominio, también con tu permiso.

### 8.2 Docker y Docker Compose
- Imagen `ghcr.io/miausalvaje/gordurasfm` multiarquitectura (amd64 y arm64), `FROM scratch` + binario estático: unos 20-30 MB (estimado). Volumen `/data` y usuario no root.
- La construye GitHub Actions con `buildx`. El `compose.yaml` de ejemplo lleva Navidrome + GordurasFM, con `ND_LISTENBRAINZ_BASEURL` ya puesto.

### 8.3 Android con Termux (con honestidad)
- ❌ **Docker no**: necesita funciones del kernel que Android no tiene (o no deja usar sin root).
- ✅ **Opción recomendada: binario nativo `aarch64-musl`.** Como es estático y trae sus propios certificados, zonas horarias y resolutor DNS, funciona en Termux sin instalar nada. El DNS se lee de `$PREFIX/etc/resolv.conf` y, si no está, se usa 1.1.1.1. Se arranca con `termux-services` (runit) o Termux:Boot, junto a `termux-wake-lock`.
- ⚠️ Android puede matar Termux aunque esté en segundo plano (ahorro de batería). Hay que quitarle la optimización de batería a Termux. Aun así, un móvil no da la fiabilidad 24/7 de un LXC. Navidrome reintenta las escuchas que fallan, así que un corte corto no pierde datos.
- Alternativa: `proot-distro` con Ubuntu y el mismo binario. Va más lento y no aporta nada, salvo que se quiera la misma estructura que en el LXC. Se documenta, pero no es la recomendada.
- Encaja con tu Navidrome del móvil (Termux, `korvilash.top`): GordurasFM se ejecutaría a su lado en `127.0.0.1` y ese Navidrome lo usaría como `BaseURL`.

---

## 9. Conservación del historial y de los Wrapped

- Las escuchas **nunca se borran solas**. Solo el propio usuario puede borrar una escucha o todo su historial (con confirmación).
- Se guarda el JSON original de cada escucha, así que una mejora de la normalización se puede aplicar a todo el historial sin perder nada.
- Migraciones solo hacia delante, con copia automática de la BD antes de aplicar una nueva.
- **Copia diaria** con `VACUUM INTO` a `/var/lib/gordurasfm/backups/` (se guardan 7 diarias y 4 semanales), además de la copia de Proxmox del LXC.
- **Exportación** por usuario (`/api/v1/export`): todo su historial en JSON y CSV, en el mismo formato que se puede volver a importar.
- Wrapped: filas inmutables con número de versión. Se guardan copias propias de las imágenes en `images/`, que no se borran mientras algún Wrapped las use.

---

## 10. Entorno de desarrollo ✅

- **LXC 135 `gordurasfm-dev`** (192.168.1.46), Debian 13, sin privilegios: 8 vCPU, 8 GB de RAM, 40 GB en `thinpool-nvme`, sin arranque automático.
- Usuario `dev` con Rust estable, `cargo-zigbuild` + zig (binarios musl x86_64 y aarch64), Flutter estable y **Navidrome 0.64.2 de pruebas** en `~/navidrome-pruebas` (binario de Go, sin Docker).
- El código se edita en `~/GordurasFM` del LXC 111 y se copia con `./sync.sh` (rsync). La prueba de principio a fin con Navidrome real es `scripts/e2e-navidrome.sh`.

---

## 11. Fases

| Fase | Entrega | Cómo se comprueba |
|---|---|---|
| 1 | Backend base: config, BD y migraciones, cifrado, login por Subsonic, sesiones, tokens, `/1/validate-token`, `/1/submit-listens`, paso a LB, cola de reenvío a ListenBrainz | Tests unitarios + integración (BD en memoria, ListenBrainz y Subsonic falsos) + Navidrome 0.64.2 de pruebas enviando escuchas de verdad |
| 2 | API de estadísticas (periodos, gráfica, tops, caché), Last.fm (conexión, reenvío, importación reanudable, CSV/JSON), `enricher`, proxy de portadas | Tests de periodos con cambio de hora, duplicados e importación cortada y reanudada |
| 3 | Flutter: login, portada de estadísticas, Top Albums, Top Tracks, artistas, historial, perfil y conexiones | Revisión visual en móvil (390×844) y escritorio |
| 4 | Wrapped: generador, foto fija, Stories, 3 pruebas, música y exportación | Generar un Wrapped real a partir de un historial importado |
| 5 | `install.sh` + systemd, Docker multiarquitectura, guía de Termux, documentación de la API, CI | Instalación limpia en un LXC nuevo, medida de RAM y CPU reales |

Estado: fases 1 y 2 terminadas (2026-11-26).
