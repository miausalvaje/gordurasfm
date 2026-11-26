# Instalar GordurasFM

GordurasFM es **un solo binario estático** (Linux, musl) que trae la web, los certificados raíz, las zonas horarias y su propio resolutor DNS. No necesita base de datos aparte, ni Java, ni Node: SQLite va dentro.

| Dónde | Cómo | Guía |
|---|---|---|
| Debian/Ubuntu con systemd (LXC de Proxmox, VPS, Raspberry Pi) | `deploy/lxc/install.sh` | abajo |
| Docker / Docker Compose (amd64 y arm64) | `ghcr.io/miausalvaje/gordurasfm` | abajo |
| Android con Termux | `deploy/termux/install.sh` | [`TERMUX.md`](TERMUX.md) |

Binarios publicados en cada versión: `gordurasfm-x86_64-linux-musl`, `gordurasfm-aarch64-linux-musl` y `SHA256SUMS`.

## 1. LXC / servidor con systemd

Recursos: 1 vCPU, 512 MB de RAM y 2-4 GB de disco sobran (medidas reales en la sección 6). Funciona en un LXC **sin privilegios**.

```sh
curl -fsSL https://raw.githubusercontent.com/miausalvaje/gordurasfm/main/deploy/lxc/install.sh \
  | sudo bash -s -- --navidrome-url http://IP-DE-NAVIDROME:4533
```

En una plantilla mínima sin `curl` (la de Ubuntu de Proxmox no lo trae): `apt install -y curl` antes.

Opciones: `--version vX.Y.Z`, `--binary RUTA` (binario propio, sin descargar nada), `--public-url URL`, `--port N`, `--no-start`.

Qué hace:
1. Descarga el binario de tu arquitectura y comprueba su SHA-256.
2. Crea el usuario de sistema `gordurasfm`, `/var/lib/gordurasfm` (BD, imágenes y copias) y `/etc/gordurasfm/config.toml`.
3. Genera la clave maestra `/etc/gordurasfm/master.key` (cifra los tokens y contraseñas guardados: **haz copia de ella**; sin ella no se pueden leer).
4. Instala y arranca la unidad `gordurasfm.service` (aislada: `ProtectSystem=strict`, solo puede escribir en `/var/lib/gordurasfm`).
5. Instala `gordurasfm-update`.

Archivos:

| Ruta | Qué es |
|---|---|
| `/usr/local/bin/gordurasfm` | el programa |
| `/etc/gordurasfm/config.toml` | configuración (opciones en [`server/config.example.toml`](../server/config.example.toml)) |
| `/etc/gordurasfm/master.key` | clave maestra |
| `/var/lib/gordurasfm/gordurasfm.db` | la BD (SQLite en modo WAL) |
| `/var/lib/gordurasfm/backups/` | copias automáticas |

Registros: `journalctl -u gordurasfm -f`.

### Actualizar

```sh
sudo gordurasfm-update                 # última versión
sudo gordurasfm-update --version v0.1.1
```

Antes de cambiar el binario guarda una copia de la BD (`backups/manual-*.db`). Si la versión nueva no arranca en 30 s, vuelve a poner la anterior. Si además la nueva había migrado la BD, la anterior no arrancará con ella: restaura la copia (sección 5).

### Desinstalar

```sh
sudo systemctl disable --now gordurasfm
sudo rm /etc/systemd/system/gordurasfm.service /usr/local/bin/gordurasfm /usr/local/sbin/gordurasfm-update
# Datos (¡irrecuperable!): sudo rm -r /var/lib/gordurasfm /etc/gordurasfm && sudo userdel gordurasfm
```

## 2. Docker

Imagen `FROM scratch` (solo el binario), usuario sin privilegios (65532), volumen `/data`:

```sh
docker run -d --name gordurasfm -p 8090:8090 \
  -e GFM_NAVIDROME_URL=http://IP-DE-NAVIDROME:4533 \
  -e GFM_PUBLIC_URL=http://IP-DEL-SERVIDOR:8090 \
  -v gordurasfm-data:/data --restart unless-stopped \
  ghcr.io/miausalvaje/gordurasfm:latest
```

Con Navidrome en el mismo Compose: [`deploy/docker/compose.yaml`](../deploy/docker/compose.yaml) (ya trae `ND_LISTENBRAINZ_BASEURL`).

- La configuración va por variables `GFM_*` (lista en `gordurasfm --help`). También se puede montar un `config.toml` y pasar `GFM_CONFIG=/ruta/config.toml`.
- La clave maestra se crea en `/data/master.key`. Copia de seguridad = copiar el volumen entero.
- Si montas una carpeta del host en vez de un volumen con nombre: `sudo chown 65532:65532 carpeta`.
- `docker exec gordurasfm /gordurasfm backup` hace una copia al momento en `/data/backups`.
- Tiene `HEALTHCHECK` (`gordurasfm healthcheck`).

## 3. Conectar Navidrome

1. En `navidrome.toml` (o variables):
   ```toml
   ListenBrainz.Enabled = true
   ListenBrainz.BaseURL = "http://IP-DE-GORDURASFM:8090/1/"
   ```
   o `ND_LISTENBRAINZ_ENABLED=true` y `ND_LISTENBRAINZ_BASEURL=http://IP-DE-GORDURASFM:8090/1/`. Reinicia Navidrome.
2. Cada usuario entra en GordurasFM con su cuenta de Navidrome, va a *Conexiones y ajustes*, genera un **token de escucha** y lo pega en *Navidrome → Personal → ListenBrainz*.
3. Si alguien ya enviaba a ListenBrainz, que conecte su token real en GordurasFM: sus escuchas siguen llegando a ListenBrainz. Los usuarios que no usan GordurasFM no notan nada: los tokens que no son de nadie se pasan tal cual a ListenBrainz.

## 4. Acceso desde fuera (proxy inverso)

Pon GordurasFM detrás de tu proxy (Nginx Proxy Manager, Caddy, Traefik…) y cambia `public_url` a la URL `https://…`: las cookies pasan a ser `Secure` solas. No hace falta nada especial (no usa WebSocket). Ejemplo con Caddy:

```
gordurasfm.example.com {
    reverse_proxy 127.0.0.1:8090
}
```

## 5. Copias de seguridad

- **Automáticas**: una diaria y una semanal con `VACUUM INTO` (consistentes aunque esté recibiendo escuchas) en `backups/`; se guardan 7 y 4 (`[backup]` en la configuración).
- **Antes de migrar**: al arrancar una versión con cambios en la BD se guarda `backups/pre-migracion-*.db`.
- **A mano**: `sudo -u gordurasfm gordurasfm backup --config /etc/gordurasfm/config.toml` (`manual-*.db`; también la hace `gordurasfm-update`).
- De las copias a mano y de antes de migrar se guardan las 5 más nuevas de cada tipo.
- **Qué copiar fuera del servidor**: `backups/` (o la BD) **y** `master.key`.

Restaurar:

```sh
sudo systemctl stop gordurasfm
sudo -u gordurasfm cp /var/lib/gordurasfm/backups/diaria-AAAAMMDD-HHMMSS.db /var/lib/gordurasfm/gordurasfm.db
sudo rm -f /var/lib/gordurasfm/gordurasfm.db-wal /var/lib/gordurasfm/gordurasfm.db-shm
sudo systemctl start gordurasfm
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
