# API de GordurasFM

Estado: fase 1. La documentación OpenAPI interactiva (`/api/docs`) llega en la fase 5; mientras tanto, esta es la referencia.

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
