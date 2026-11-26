# GordurasFM

Estadísticas de escucha autoalojadas para los usuarios de un [Navidrome](https://www.navidrome.org/): recibe las escuchas por un endpoint compatible con ListenBrainz, las reenvía a ListenBrainz (y a Last.fm), importa el historial de Last.fm y ofrece rankings y un Wrapped anual.
Backend en Rust (axum + SQLite WAL) y web en Flutter, servidos por **un único binario estático** (x86_64 y aarch64, sin dependencias).

## Instalar

| Dónde | Cómo |
|---|---|
| Debian/Ubuntu con systemd (LXC de Proxmox, VPS, Raspberry Pi) | `curl -fsSL https://raw.githubusercontent.com/miausalvaje/gordurasfm/main/deploy/lxc/install.sh \| sudo bash -s -- --navidrome-url http://IP:4533` |
| Docker / Compose | `ghcr.io/miausalvaje/gordurasfm` · [`deploy/docker/compose.yaml`](deploy/docker/compose.yaml) |
| Android (Termux) | [`docs/TERMUX.md`](docs/TERMUX.md) |

Guía completa (Navidrome, proxy inverso, copias de seguridad, actualizar, consumo medido): [`docs/DESPLIEGUE.md`](docs/DESPLIEGUE.md).

Después, en Navidrome: `ListenBrainz.Enabled = true` y `ListenBrainz.BaseURL = "http://<gordurasfm>:8090/1/"`. Cada usuario entra en la web con su cuenta de Navidrome, genera un token de escucha y lo pega en *Navidrome → Personal → ListenBrainz*.

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 0 | Diseño: [`docs/DISENO.md`](docs/DISENO.md) | ✅ |
| 1 | Backend base: login con Navidrome, sesiones, tokens, `/1/validate-token`, `/1/submit-listens`, paso a ListenBrainz, cola de reenvío con reintentos | ✅ |
| 2 | Estadísticas por periodo (zona horaria del usuario), tops, Last.fm (conexión, reenvío, importación reanudable), importar/exportar ficheros, portadas desde Navidrome | ✅ |
| 3 | Web: login, inicio, tops, historial, perfil, conexiones, administración | ✅ |
| 4 | Wrapped: foto fija anual con versiones, 15 diapositivas, 3 pruebas, música de Navidrome, exportar a PNG/ZIP | ✅ |
| 5 | Instalador para LXC/systemd, Docker multiarquitectura, Termux, copias automáticas de la BD, OpenAPI | ✅ |

## API

- Guía: [`docs/API.md`](docs/API.md).
- OpenAPI 3.1: [`docs/openapi.json`](docs/openapi.json), también en `GET /api/v1/openapi.json` de cada instalación (ábrelo en cualquier visor de OpenAPI).

## Desarrollo

```sh
cd server
cargo test                      # unitarios + integración (Navidrome y ListenBrainz falsos)
cargo clippy --all-targets -- -D warnings
GFM_NAVIDROME_URL=http://navidrome:4533 GFM_DATA_DIR=./data cargo run
../scripts/e2e-navidrome.sh     # de principio a fin con un Navidrome real
../scripts/e2e-carga.sh 100000  # carga: importa 100 000 escuchas y mide tiempos y memoria
LASTFM_API_KEY=… ../scripts/e2e-lastfm-real.sh <usuario>   # importación real desde Last.fm
cd ../web && flutter test && flutter analyze
../scripts/build-release.sh     # web + binarios estáticos x86_64/aarch64 en dist/ (Flutter, zig y cargo-zigbuild)
../scripts/check-deploy.sh      # comprueba instaladores y unidad systemd
```

Publicar una versión: sube la versión en `server/Cargo.toml` y en `docs/openapi.json`, y crea la etiqueta `vX.Y.Z`. La CI (`.github/workflows/release.yml`) compila, prueba el instalador y la imagen, crea la versión en GitHub y sube la imagen a `ghcr.io`.

Configuración: [`server/config.example.toml`](server/config.example.toml) o variables `GFM_*` (`gordurasfm --help`).

## Licencia

[AGPL-3.0](LICENSE).
