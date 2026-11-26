# API de GordurasFM

Estado: fase 2. La documentación OpenAPI interactiva (`/api/docs`) llega en la fase 5; mientras tanto, esta es la referencia.

## Autenticación

- **Web**: `POST /api/v1/auth/login` deja una cookie `gfm_session` (`HttpOnly`, `SameSite=Lax`, 30 días, se renueva con el uso).
  Las peticiones que cambian algo (`POST`, `PUT`, `PATCH`, `DELETE`) con cookie deben llevar la cabecera `X-Requested-With` (con cualquier valor): protección CSRF.
- **Integraciones**: `Authorization: Bearer <token>` con un token de tipo `api` (se crea en `POST /api/v1/tokens`). No necesita `X-Requested-With`.
- Errores: `{"error": "codigo", "message": "texto en español"}` con `400`, `401`, `403`, `404`, `502` o `500`.

## Compatibilidad con ListenBrainz (`/1/`)

Para Navidrome: `ND_LISTENBRAINZ_BASEURL=http://<gordurasfm>:8090/1/`. Cada usuario pega en *Navidrome → Personal → ListenBrainz* su token de escucha de GordurasFM, o su token real de ListenBrainz si ya lo tiene conectado en GordurasFM.

| Método y ruta | Qué hace |
|---|---|
| `GET /1/validate-token` | `Authorization: Token <t>` (o `?token=`). Responde `{"code":200,"message":"Token valid.","valid":true,"user_name":"…"}` |
| `POST /1/submit-listens` | `listen_type`: `single` (1 escucha), `import` (1-1000) o `playing_now` (1, sin `listened_at`). Responde `{"status":"ok"}`. Si la BD no está disponible, `503` (Navidrome lo reintenta) |
| `GET /1/<cualquier otra>` | Se pasa tal cual a la API real de ListenBrainz (Navidrome pide ahí metadatos de artistas) |

Los tokens que no son de nadie se pasan tal cual a ListenBrainz (`listenbrainz.passthrough_unknown_tokens`).

Reglas al guardar: misma canción del mismo usuario a ±60 s = duplicada; pistas de menos de 30 s (si se conoce la duración) no cuentan; `listened_at` anterior a octubre de 2002 o más de 10 min en el futuro se rechaza.

## API propia (`/api/v1`)

| Método y ruta | Cuerpo / parámetros | Respuesta |
|---|---|---|
| `GET /api/v1/salud` | — | `{"status":"ok","version":"…","db":true}` |
| `POST /api/v1/auth/login` | `{"username","password"}` (de Navidrome) | `{"user":{…}}` + cookie. `401` si Navidrome rechaza las credenciales; `403` si la cuenta espera aprobación |
| `POST /api/v1/auth/logout` | — | `{"ok":true}` y borra la cookie |
| `GET /api/v1/me` | — | `{id, username, display_name, is_admin, is_approved, timezone, is_public, created_at}` |
| `PATCH /api/v1/me` | `{"display_name"?, "timezone"?, "is_public"?}` | el usuario actualizado |
| `GET /api/v1/tokens` | — | `[{id, kind, hint, label, created_at, last_used_at}]` (el token nunca se vuelve a enseñar) |
| `POST /api/v1/tokens` | `{"kind":"scrobble"\|"api","label"?}` | `{id, kind, token}` (solo esta vez) |
| `DELETE /api/v1/tokens/{id}` | — | `{"ok":true}` |
| `GET /api/v1/connections` | — | `[{provider, remote_username, forward_enabled, status, last_error, updated_at}]` |
| `PUT /api/v1/connections/listenbrainz` | `{"token"}` | la conexión. Comprueba el token con ListenBrainz, lo guarda cifrado y reanuda lo pendiente |
| `PATCH /api/v1/connections/{provider}` | `{"forward_enabled": bool}` | la conexión. Al desactivar, lo pendiente queda en pausa |
| `DELETE /api/v1/connections/{provider}` | — | `{"ok":true}`; borra también lo que quedaba por reenviar a ese destino |
| `GET /api/v1/forward/status` | — | `[{target, status, count, last_error}]`, estados `pending`, `done`, `paused`, `failed` |
| `POST /api/v1/forward/retry` | — | `{"requeued": n}` |
| `GET /api/v1/scrobbles` | `?limit=50&before=<unix>` | `{"total", "items":[{id, played_at, artist, track, album, duration_ms, source}], "next_before"}` |
| `GET /api/v1/now-playing` | — | lo que está sonando o `null` |

### Estadísticas (fase 2)

Periodos (`period`): `this_week`, `last_week`, `this_month`, `last_month`, `this_year`, `last_year`, `all_time`. Se calculan con la zona horaria del usuario (`PATCH /api/v1/me`), con la semana desde el lunes. Los límites se calculan en hora local, así que los cambios de hora no rompen los días.

| Método y ruta | Parámetros | Respuesta |
|---|---|---|
| `GET /api/v1/stats` | `?period=this_week&limit=10` | `{period, chart, totals, top_albums, top_tracks, top_artists}` |
| `GET /api/v1/top/{albums\|tracks\|artists}` | `?period=&limit=50&page=1` | `{period, page, limit, total, items}` |

- `period`: `{key, timezone, from, to, unit, previous}`. `from`/`to` en segundos Unix (UTC), `to` excluido. En los periodos en curso, `to` es el final del periodo. `unit`: `day` (semanas y meses), `month` (años y «siempre» de hasta 36 meses), `year`. `previous` es el periodo con el que se compara; en los periodos en curso se recorta al mismo tiempo transcurrido.
- `chart`: `[{label, start, end, plays}]`. `label`: `2026-11-23`, `2026-11` o `2026`, en hora local.
- `totals`: `plays`, `listening_ms` (solo cuenta las escuchas con duración conocida), `plays_without_duration`, `artists`, `albums`, `tracks`, `avg_plays_per_day`, `top_day {date, plays}`, `previous_plays`, `change_pct`, `by_hour[24]`, `by_weekday[7]` (0 = lunes).
- Elementos de los tops: `rank` (los empates comparten posición: 1, 2, 2, 4), `plays`, `last_played` y:
  - álbumes: `id, title, artist, cover`
  - canciones: `id, title, artist, album, album_id, cover, duration_ms` (el álbum con el que más se ha escuchado)
  - artistas: `id, name, image, covers[]` (hasta 4 portadas para hacer un collage si no hay imagen)

### Imágenes

| Método y ruta | Qué hace |
|---|---|
| `GET /api/v1/media/cover/{album_id}?size=300` | Portada del álbum, sacada de Navidrome (con tus permisos) o de Last.fm, y guardada en caché. Solo de álbumes que has escuchado |
| `GET /api/v1/media/artist/{artist_id}?size=300` | Imagen del artista desde Navidrome |
| `GET /api/v1/media/stream/{song_id}` | Audio de Navidrome (MP3 a 192 kbit/s si Navidrome puede convertirlo) para la música del Wrapped. Admite `Range` y no se comprime. Solo canciones que has escuchado |

Tamaños: 64, 150, 300, 600 y 1000 (se redondea al más cercano). Si no hay imagen, `404`; se vuelve a intentar al día siguiente.

### Wrapped (fase 4)

| Método y ruta | Cuerpo / parámetros | Respuesta |
|---|---|---|
| `GET /api/v1/wrapped` | — | `{current_year, years:[{year, plays, current, wrapped: {id, version, is_final, generated_at, outdated} \| null}]}` (solo años con escuchas) |
| `GET /api/v1/wrapped/{year}` | — | `{id, year, version, is_final, generated_at, outdated, answers, data}`. Si no existe, se genera: el año en curso como vista previa (`version 0`, se rehace como mucho cada hora) y los pasados como definitivo (`version 1`). `404` si el año no tiene escuchas o es futuro |
| `POST /api/v1/wrapped/{year}/regenerate` | — | igual que el anterior. En años pasados crea una versión nueva y conserva las anteriores |
| `POST /api/v1/wrapped/answers/{id}` | `{question: "artist"\|"album"\|"track", chosen_rank}` | `{correct, answers}`. Solo cuenta la primera respuesta de cada prueba |
| `GET /api/v1/wrapped/image/{hash}` | — | imagen guardada para el Wrapped (copia propia; sigue aunque cambie la biblioteca). Solo si sale en algún Wrapped tuyo |

- `data` es una foto fija: `{schema, year, user, timezone, totals {plays, ms, ms_estimated, artists, albums, tracks, days_with_music, top_day, peak_hour, first_play, previous_plays, change_pct}, top_artists[5] {rank, name, plays, image, covers[]}, top_albums[5] {rank, title, artist, plays, cover}, top_tracks[5] {rank, title, artist, album, plays, cover}, monthly[12] {month, plays, top_artist}, evolution[4] {quarter, plays, top_artists[3], image, covers}, top_month {month, plays, top_artist, top_track}, soundtrack[≤12] {navidrome_song_id, title, artist}, generated_at}`.
- Las imágenes van como `img:<sha256>` → `/api/v1/wrapped/image/<sha256>`.
- `outdated`: el definitivo tiene menos (o más) escuchas que las que hay ahora en la BD para ese año (por ejemplo, tras importar de Last.fm). La web ofrece «Rehacer».
- El definitivo del año anterior se genera solo cada hora a partir del 1 de enero a las 06:00 (hora del usuario).

### Last.fm

| Método y ruta | Qué hace |
|---|---|
| `GET /api/v1/lastfm/status` | `{own_key, instance_key, available}` |
| `GET /api/v1/connections/lastfm/start` | `{url}`: abre esa URL para autorizar en Last.fm. Al volver, redirige a `/ajustes/conexiones?lastfm=ok` (o `?lastfm=error&motivo=…`) |
| `GET /api/v1/connections/lastfm/callback` | La usa Last.fm al volver (`state` + `token`) |
| `PUT /api/v1/connections/lastfm/key` | `{api_key, api_secret}` propios (opcional) |
| `DELETE /api/v1/connections/lastfm/key` | Vuelve a la key de la instancia |
| `PATCH /api/v1/connections/lastfm` | `{forward_enabled}`. El reenvío a Last.fm **empieza apagado** |

Reenvío a Last.fm: lotes de 50 (`track.scrobble`). Last.fm no acepta escuchas de hace más de 14 días: esas quedan `dead`. Si el usuario ya tiene en Navidrome el envío a Last.fm, no debe activar también este, porque se duplicarían.

### Importar y exportar

| Método y ruta | Cuerpo | Respuesta |
|---|---|---|
| `POST /api/v1/import/lastfm` | `{"mode": "full"\|"new", "username"?}` | el trabajo. `new` trae solo lo posterior a la última escucha importada (menos 1 h de margen) |
| `POST /api/v1/import/file` | multipart, campo `file` (hasta 200 MB, UTF-8) | `{job, format, listens, invalid}` |
| `GET /api/v1/import/jobs` | — | los últimos 50 trabajos |
| `GET /api/v1/import/jobs/{id}` | — | `{id, kind, status, progress_done, progress_total, stats {stored, duplicates, ignored, invalid}, error}` |
| `POST /api/v1/import/jobs/{id}/cancel` | — | |
| `POST /api/v1/import/jobs/{id}/resume` | — | sigue un trabajo `failed` o `cancelled` por donde iba |
| `GET /api/v1/export?format=json\|csv` | — | todo tu historial, en un formato que se puede volver a importar |

Formatos de fichero:
- CSV de *lastfm-to-csv* sin cabecera: `artist,album,track,date` (fecha «14 Nov 2023 22:13», UTC).
- CSV con cabecera: columnas `artist`, `track`/`name`/`title`, `album`, y fecha en `uts`/`played_at`/`timestamp`/`date`/`utc_time` (Unix, RFC 3339 o el formato de Last.fm). La exportación propia de GordurasFM también sirve.
- JSON: lista o una escucha por línea; formato de ListenBrainz (`listened_at` + `track_metadata`) o el propio (`played_at, artist, track, album`).

Solo puede haber una importación en marcha por usuario. Las escuchas importadas no se reenvían a ningún sitio.

### Administración (solo admin)

| Método y ruta | Qué hace |
|---|---|
| `GET /api/v1/admin/lastfm` | `{configured, source: "config"\|"settings"\|"none", api_key}` |
| `PUT /api/v1/admin/lastfm` | `{api_key, api_secret}`: key de la instancia (el secret se guarda cifrado) |
| `DELETE /api/v1/admin/lastfm` | |
| `GET /api/v1/admin/users` | usuarios con su número de escuchas |
| `PATCH /api/v1/admin/users/{id}` | `{is_approved?, is_admin?}`. Al quitar la aprobación se cierran sus sesiones |
