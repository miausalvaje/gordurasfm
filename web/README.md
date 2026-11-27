# Web (Flutter)

Interfaz de OwnFM en Flutter Web. El binario del servidor la embebe desde `web/build/web`
(si no existe, `server/build.rs` pone una página provisional).

- Compilar: `scripts/build-web.sh` (sin CDN: CanvasKit y fuentes van dentro del binario).
- Probar: `flutter analyze && flutter test`.
- Demo con datos inventados: `scripts/demo.sh` (Navidrome de pruebas + dos años de historial).
