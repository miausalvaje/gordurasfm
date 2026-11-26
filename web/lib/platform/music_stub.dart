import 'dart:typed_data';

/// Lista de canciones que suena de fondo sin reiniciarse entre diapositivas.
class BackgroundMusic {
  BackgroundMusic(List<String> urls);

  bool get muted => true;
  bool get available => false;

  /// Empieza a sonar. Hay que llamarla dentro de un toque del usuario (los navegadores bloquean el audio
  /// automático).
  void start() {}
  void setMuted(bool m) {}
  void pause() {}
  void resume() {}
  void dispose() {}
}

/// Comparte el fichero (Web Share API) si el navegador puede; si no, lo descarga. Devuelve true si se compartió.
Future<bool> shareOrDownload(Uint8List bytes, String filename, String mime, {bool share = true}) async => false;
