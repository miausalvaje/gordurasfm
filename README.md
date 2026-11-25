# GordurasFM

Estadísticas de escucha autoalojadas para los usuarios de un [Navidrome](https://www.navidrome.org/): recibe las escuchas por un endpoint compatible con ListenBrainz, las reenvía a ListenBrainz (y a Last.fm), importa el historial de Last.fm y ofrece rankings y un Wrapped anual.
Backend en Rust (axum + SQLite WAL) y web en Flutter, servidos por **un único binario estático**.

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 0 | Diseño: [`docs/DISENO.md`](docs/DISENO.md) | ✅ |
| 1 | Backend base: login con Navidrome, sesiones, tokens, `/1/validate-token`, `/1/submit-listens`, paso a ListenBrainz, cola de reenvío con reintentos | ✅ |
| 2 | Estadísticas, Last.fm (conexión, reenvío, importación) | ⏳ |
| 3 | Web: login, perfil, estadísticas, tops | ⏳ |
| 4 | Wrapped | ⏳ |
| 5 | Instalador LXC, Docker, Termux, OpenAPI | ⏳ |

API: [`docs/API.md`](docs/API.md).

## Probarlo (fase 1)

```sh
cd server
cargo build --release
GFM_NAVIDROME_URL=http://navidrome:4533 GFM_DATA_DIR=./data ./target/release/gordurasfm
```

En Navidrome:
1. `ND_LISTENBRAINZ_ENABLED=true` y `ND_LISTENBRAINZ_BASEURL=http://<gordurasfm>:8090/1/`.
2. Cada usuario entra en GordurasFM con su cuenta de Navidrome, crea un token de escucha (`POST /api/v1/tokens` con `{"kind":"scrobble"}`) y lo pega en *Navidrome → Personal → ListenBrainz*.
3. Para seguir enviando a ListenBrainz: `PUT /api/v1/connections/listenbrainz` con su token real. Si ya tenía ese token puesto en Navidrome, no hace falta cambiar nada.

Configuración: [`server/config.example.toml`](server/config.example.toml) o variables `GFM_*`.

## Desarrollo

```sh
cd server
cargo test                      # unitarios + integración (Navidrome y ListenBrainz falsos)
cargo clippy --all-targets -- -D warnings
cargo zigbuild --release --target x86_64-unknown-linux-musl    # binario estático
cargo zigbuild --release --target aarch64-unknown-linux-musl   # Android/Termux, Raspberry Pi
../scripts/e2e-navidrome.sh     # de principio a fin con un Navidrome real
```

## Licencia

[AGPL-3.0](LICENSE).
