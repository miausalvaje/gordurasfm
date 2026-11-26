# GordurasFM en Android con Termux

Se usa el **binario nativo `aarch64`**. Es estático y trae sus certificados, zonas horarias y resolutor DNS, así que funciona en Termux sin instalar nada más (ni `proot`, ni Docker).

> ❌ **Docker no funciona en Android** sin root: necesita funciones del kernel que Android no deja usar.
>
> ⚠️ Un móvil no es un servidor 24/7: Android puede cerrar Termux para ahorrar batería. Navidrome reintenta las escuchas que no ha podido enviar, así que un corte corto no pierde datos, pero uno largo sí puede. Si tienes un ordenador o un LXC siempre encendido, mejor ahí.

## Requisitos

- Móvil con procesador de 64 bits ARM (`aarch64`; lo son casi todos desde 2017). Compruébalo con `uname -m`.
- [Termux](https://f-droid.org/packages/com.termux/) de F-Droid o GitHub (la versión de Google Play está abandonada).
- Opcional: [Termux:Boot](https://f-droid.org/packages/com.termux.boot/), para arrancar al encender el móvil.

## Instalar

```sh
pkg install -y curl
curl -fsSL https://raw.githubusercontent.com/miausalvaje/gordurasfm/main/deploy/termux/install.sh | bash
```

Opciones: `--navidrome-url URL` (por defecto `http://127.0.0.1:4533`, Navidrome en el mismo móvil), `--port N`, `--version vX.Y.Z`, `--binary RUTA`.

El script:
1. Instala `termux-services` si falta.
2. Descarga `gordurasfm-aarch64-linux-musl`, comprueba su SHA-256 y lo deja en `$PREFIX/bin/gordurasfm`.
3. Crea `~/.gordurasfm/config.toml` y la carpeta de datos `~/.gordurasfm/data` (BD, clave maestra, copias).
4. Crea el servicio `gordurasfm` (runit) con `termux-wake-lock`, para que Android no duerma la CPU.

La primera vez que instalas `termux-services` hay que **cerrar Termux del todo y volver a abrirlo**; después:

```sh
sv-enable gordurasfm      # arrancar y que arranque con Termux
sv status gordurasfm
tail -f $PREFIX/var/log/sv/gordurasfm/current
```

## Que no lo mate Android

1. *Ajustes → Aplicaciones → Termux → Batería → Sin restricciones* (el nombre cambia según la marca; en Xiaomi, Huawei y Samsung hay además un «inicio automático» que activar). <https://dontkillmyapp.com> tiene los pasos de cada marca.
2. Deja la notificación de Termux con *Acquire wakelock* (lo hace el servicio con `termux-wake-lock`).

## Arrancar al encender el móvil (Termux:Boot)

Abre Termux:Boot una vez y crea el script:

```sh
mkdir -p ~/.termux/boot
cat > ~/.termux/boot/start-services <<'EOF'
#!/data/data/com.termux/files/usr/bin/sh
termux-wake-lock
. $PREFIX/etc/profile.d/start-services.sh
EOF
chmod +x ~/.termux/boot/start-services
```

## Conectar Navidrome

Si Navidrome corre en el mismo móvil, en su `navidrome.toml`:

```toml
ListenBrainz.Enabled = true
ListenBrainz.BaseURL = "http://127.0.0.1:8090/1/"
```

Desde otros dispositivos de la red se entra a la web con `http://IP-DEL-MÓVIL:8090` (cambia `public_url` en `~/.gordurasfm/config.toml` a esa dirección y reinicia con `sv restart gordurasfm`).

## DNS

Android no tiene `/etc/resolv.conf`. GordurasFM busca los servidores DNS en este orden: variable `GFM_DNS` (p. ej. `GFM_DNS=192.168.1.1`), `/etc/resolv.conf`, `$PREFIX/etc/resolv.conf` (el de Termux) y, si no hay ninguno, Cloudflare (`1.1.1.1`). `localhost` se resuelve siempre sin DNS.

## Actualizar, copias y desinstalar

- Actualizar: vuelve a lanzar el instalador (hace una copia de la BD antes de cambiar el binario).
- Copia al momento: `gordurasfm backup --config ~/.gordurasfm/config.toml`. Las automáticas (diaria y semanal) van a `~/.gordurasfm/data/backups`. Para sacarlas del móvil: `termux-setup-storage` y copia esa carpeta, **con `master.key`**, a `~/storage/shared/`.
- Desinstalar: `sv-disable gordurasfm && rm -r $PREFIX/var/service/gordurasfm $PREFIX/bin/gordurasfm` (y `~/.gordurasfm` si quieres borrar los datos).

## Alternativa: proot-distro

También funciona el mismo binario dentro de `proot-distro` (Ubuntu), pero va más lento y no aporta nada: no la recomiendo.
