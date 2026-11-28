# Instalar OwnFM

OwnFM es **un solo binario estático** (Linux, musl) que trae la web, los certificados raíz, las zonas horarias y su propio resolutor DNS. No necesita base de datos aparte, ni Java, ni Node: SQLite va dentro.

| Dónde | Cómo | Guía |
|---|---|---|
| Debian/Ubuntu con systemd (LXC de Proxmox, VPS, Raspberry Pi) | `deploy/lxc/install.sh` | abajo |
| Docker / Docker Compose (amd64 y arm64) | `ghcr.io/miausalvaje/ownfm` | abajo |
| Android con Termux | `deploy/termux/install.sh` | [`TERMUX.md`](TERMUX.md) |

Binarios publicados en cada versión: `ownfm-x86_64-linux-musl`, `ownfm-aarch64-linux-musl` y `SHA256SUMS`.

## 1. LXC / servidor con systemd

Recursos: 1 vCPU, 512 MB de RAM y 2-4 GB de disco sobran (medidas reales en la sección 6). Funciona en un LXC **sin privilegios**.

```sh
curl -fsSL https://raw.githubusercontent.com/miausalvaje/ownfm/main/deploy/lxc/install.sh \
  | sudo bash -s -- --navidrome-url http://IP-DE-NAVIDROME:4533
```

En una plantilla mínima sin `curl` (la de Ubuntu de Proxmox no lo trae): `apt install -y curl` antes.

Opciones: `--version vX.Y.Z`, `--binary RUTA` (binario propio, sin descargar nada), `--public-url URL`, `--port N`, `--no-start`.

Qué hace:
1. Descarga el binario de tu arquitectura y comprueba su SHA-256.
2. Crea el usuario de sistema `ownfm`, `/var/lib/ownfm` (BD, imágenes y copias) y `/etc/ownfm/config.toml`.
3. Genera la clave maestra `/etc/ownfm/master.key` (cifra los tokens y contraseñas guardados: **haz copia de ella**; sin ella no se pueden leer).
4. Instala y arranca la unidad `ownfm.service` (aislada: `ProtectSystem=strict`, solo puede escribir en `/var/lib/ownfm`).
5. Instala `ownfm-update`.

Archivos:

| Ruta | Qué es |
|---|---|
| `/usr/local/bin/ownfm` | el programa |
| `/etc/ownfm/config.toml` | configuración (opciones en [`server/config.example.toml`](../server/config.example.toml)) |
| `/etc/ownfm/master.key` | clave maestra |
| `/var/lib/ownfm/ownfm.db` | la BD (SQLite en modo WAL) |
| `/var/lib/ownfm/backups/` | copias automáticas |

Registros: `journalctl -u ownfm -f`.

### Actualizar

```sh
sudo ownfm-update                 # última versión
sudo ownfm-update --version v0.1.1
```

Antes de cambiar el binario guarda una copia de la BD (`backups/manual-*.db`). Si la versión nueva no arranca en 30 s, vuelve a poner la anterior. Si además la nueva había migrado la BD, la anterior no arrancará con ella: restaura la copia (sección 5).

### Desinstalar

```sh
sudo systemctl disable --now ownfm
sudo rm /etc/systemd/system/ownfm.service /usr/local/bin/ownfm /usr/local/sbin/ownfm-update
# Datos (¡irrecuperable!): sudo rm -r /var/lib/ownfm /etc/ownfm && sudo userdel ownfm
```

## 2. Docker

Imagen `FROM scratch` (solo el binario), usuario sin privilegios (65532), volumen `/data`:

```sh
docker run -d --name ownfm -p 8090:8090 \
  -e OWNFM_NAVIDROME_URL=http://IP-DE-NAVIDROME:4533 \
  -e OWNFM_PUBLIC_URL=http://IP-DEL-SERVIDOR:8090 \
  -v ownfm-data:/data --restart unless-stopped \
  ghcr.io/miausalvaje/ownfm:latest
```

Con Navidrome en el mismo Compose: [`deploy/docker/compose.yaml`](../deploy/docker/compose.yaml) (ya trae `ND_LISTENBRAINZ_BASEURL`).

- La configuración va por variables `OWNFM_*` (lista en `ownfm --help`). También se puede montar un `config.toml` y pasar `OWNFM_CONFIG=/ruta/config.toml`.
- La clave maestra se crea en `/data/master.key`. Copia de seguridad = copiar el volumen entero.
- Si montas una carpeta del host en vez de un volumen con nombre: `sudo chown 65532:65532 carpeta`.
- `docker exec ownfm /ownfm backup` hace una copia al momento en `/data/backups`.
- Tiene `HEALTHCHECK` (`ownfm healthcheck`).

## 3. Conectar Navidrome

1. En `navidrome.toml` (o variables):
   ```toml
   ListenBrainz.Enabled = true
   ListenBrainz.BaseURL = "http://IP-DE-OWNFM:8090/1/"
   ```
   o `ND_LISTENBRAINZ_ENABLED=true` y `ND_LISTENBRAINZ_BASEURL=http://IP-DE-OWNFM:8090/1/`. Reinicia Navidrome.
2. Cada usuario entra en OwnFM con su cuenta de Navidrome, va a *Conexiones y ajustes*, genera un **token de escucha** y lo pega en *Navidrome → Personal → ListenBrainz*.
3. Si alguien ya enviaba a ListenBrainz, que conecte su token real en OwnFM: sus escuchas siguen llegando a ListenBrainz. Los usuarios que no usan OwnFM no notan nada: los tokens que no son de nadie se pasan tal cual a ListenBrainz.

### Escuchar con otro reproductor (MusicBee, etc.)

Si alguien escucha con un reproductor que no pasa por Navidrome (por ejemplo MusicBee con la música en un SMB) pero que scrobblea a Last.fm, puede activar **Sincronizar con Last.fm automáticamente** en *Conexiones y ajustes → Importar historial*. OwnFM trae lo nuevo de Last.fm cada 5 minutos (`lastfm.sync_interval_secs`, o `OWNFM_LASTFM_SYNC_INTERVAL_SECS`; 0 la desactiva). Hace falta la API key de Last.fm de la instancia (o una propia). Antes, conviene hacer una vez *Importar todo de Last.fm* para el historial anterior. Las portadas salen de Last.fm, salvo que el mismo Navidrome indexe esa música (entonces se emparejan por artista, título y álbum).

## 4. Acceso desde fuera (proxy inverso)

Pon OwnFM detrás de tu proxy (Nginx Proxy Manager, Caddy, Traefik…) y cambia `public_url` a la URL `https://…`: las cookies pasan a ser `Secure` solas. No hace falta nada especial (no usa WebSocket). Ejemplo con Caddy:

```
ownfm.example.com {
    reverse_proxy 127.0.0.1:8090
}
```

Antes de abrirlo a Internet:

- **`trusted_proxies`** con la IP (o red) del proxy, p. ej. `trusted_proxies = ["192.168.1.248"]`. El inicio de sesión admite 10 intentos cada 15 min por IP y por usuario (cada intento es una consulta a Navidrome, así que sin límite serviría para adivinar contraseñas). Sin `trusted_proxies`, todo el mundo llega con la IP del proxy y comparte contador; OwnFM solo cree `X-Forwarded-For` si la conexión viene de un proxy de confianza.
- **`/1/`** (lo que usa Navidrome para mandar escuchas) no hace falta fuera si Navidrome llega a OwnFM por la red local: ciérralo en el proxy. Con Nginx Proxy Manager, en *Advanced*: `location ^~ /1/ { return 404; }`. Si lo dejas abierto, cualquiera puede usar OwnFM para pasar peticiones a ListenBrainz (`passthrough_unknown_tokens`).
- El proxy debe mandar `X-Forwarded-For` (Nginx Proxy Manager y Caddy lo hacen por defecto).
- OwnFM ya responde con `X-Frame-Options: DENY`, `Content-Security-Policy: frame-ancestors 'none'`, `X-Content-Type-Options: nosniff` y `Referrer-Policy: same-origin`; HSTS ponlo en el proxy.

## 5. Copias de seguridad

- **Automáticas**: una diaria y una semanal con `VACUUM INTO` (consistentes aunque esté recibiendo escuchas) en `backups/`; se guardan 7 y 4 (`[backup]` en la configuración).
- **Antes de migrar**: al arrancar una versión con cambios en la BD se guarda `backups/pre-migracion-*.db`.
- **A mano**: `sudo -u ownfm ownfm backup --config /etc/ownfm/config.toml` (`manual-*.db`; también la hace `ownfm-update`).
- De las copias a mano y de antes de migrar se guardan las 5 más nuevas de cada tipo.
- **Qué copiar fuera del servidor**: `backups/` (o la BD) **y** `master.key`.

Restaurar:

```sh
sudo systemctl stop ownfm
sudo -u ownfm cp /var/lib/ownfm/backups/diaria-AAAAMMDD-HHMMSS.db /var/lib/ownfm/ownfm.db
sudo rm -f /var/lib/ownfm/ownfm.db-wal /var/lib/ownfm/ownfm.db-shm
sudo systemctl start ownfm
```

Además, cada usuario puede exportar todo su historial desde la web (*Conexiones y ajustes → Exportar mi historial*) en un formato que se puede volver a importar.

## 6. Consumo medido

Medido en un LXC nuevo de Proxmox (Ubuntu 24.04, 1 vCPU, 512 MB), binario `x86_64-linux-musl` de la versión 0.1.0:

| Situación | Medida |
|---|---|
| Instalación (`install.sh --binary`) | 1,7 s |
| Recién arrancado, en reposo | 18 MB de RSS (15 MB del cgroup) |
| CPU en reposo | 4 ms por minuto |
| Importar un CSV de 100 000 escuchas (6,2 MB) | 66 s, CPU al 98 % de 1 vCPU, pico de 85 MB de RSS (104 MB del cgroup contando caché de disco) |
| Después de importar, en reposo | 45 MB de RSS |
| BD con 100 000 escuchas | 23 MB |
| Estadísticas de «todo» con 100 000 escuchas | 517 ms la primera vez, 24 ms desde la caché |
| Estadísticas de una semana / un año | 69 ms / 196 ms |
| Generar un Wrapped de un año (33 000 escuchas) | 238 ms |
| Exportar 100 000 escuchas a CSV | 618 ms |

Tiempos medidos desde otro LXC de la misma red (incluyen la red). El binario ocupa unos 30 MB porque lleva la web dentro.

## 7. Si vienes de GordurasFM

OwnFM se llamaba antes GordurasFM. Desde la v0.2.0 todo lleva el nombre nuevo: binario `ownfm`, servicio `ownfm.service`, usuario `ownfm`, carpetas `/etc/ownfm` y `/var/lib/ownfm`, BD `ownfm.db`, variables `OWNFM_*` e imagen `ghcr.io/miausalvaje/ownfm`.

- **LXC / systemd**: `sudo gordurasfm-update` (o el instalador nuevo). Detecta la instalación antigua y hace una copia de la BD con el programa antiguo. Después pasa carpetas, usuario y configuración a los nombres nuevos (la clave maestra se conserva), quita el servicio, el binario y el actualizador antiguos y arranca `ownfm.service`. Desde entonces se actualiza con `sudo ownfm-update`.
- **Docker**: cambia la imagen a `ghcr.io/miausalvaje/ownfm`. El volumen `/data` sirve tal cual: al arrancar, `gordurasfm.db` se renombra a `ownfm.db`.
- **Termux**: vuelve a lanzar el instalador. Pasa `~/.gordurasfm` a `~/.ownfm` y cambia el servicio.
- Las variables `GFM_*` siguen funcionando (con un aviso en el registro), pero mejor cámbialas a `OWNFM_*`.
- Al cambiar el nombre de la cookie, todos tienen que volver a entrar una vez en la web.
- Navidrome no cambia: `ListenBrainz.BaseURL` sigue siendo la misma dirección.
