import 'dart:async';
import 'dart:js_interop';
import 'dart:typed_data';

import 'package:web/web.dart' as web;

/// Lista de canciones que suena de fondo sin reiniciarse entre diapositivas, con fundidos.
class BackgroundMusic {
  BackgroundMusic(this._urls);

  final List<String> _urls;
  web.HTMLAudioElement? _el;
  int _i = 0;
  int _failures = 0;
  bool _muted = false;
  bool _fadingOut = false;
  Timer? _fade;
  static const _volume = 0.75;

  bool get muted => _muted;
  bool get available => _urls.isNotEmpty;

  void start() {
    if (_urls.isEmpty || _el != null) return;
    final el = web.HTMLAudioElement()
      ..preload = 'auto'
      ..volume = 0;
    el.addEventListener('ended', ((web.Event _) => _next()).toJS);
    el.addEventListener(
      'error',
      ((web.Event _) {
        // Canción que ya no está en Navidrome (o no se puede reproducir): se salta.
        _failures++;
        if (_failures < _urls.length) _next();
      }).toJS,
    );
    el.addEventListener(
      'playing',
      ((web.Event _) {
        _failures = 0;
        _fadingOut = false;
        _fadeTo(_muted ? 0 : _volume, 1500);
      }).toJS,
    );
    el.addEventListener(
      'timeupdate',
      ((web.Event _) {
        final left = el.duration - el.currentTime;
        if (!_fadingOut && left.isFinite && left < 2 && el.duration > 6) {
          _fadingOut = true;
          _fadeTo(0, 1800);
        }
      }).toJS,
    );
    _el = el;
    _play();
  }

  void _play() {
    final el = _el;
    if (el == null) return;
    el.src = _urls[_i];
    el.play().toDart.catchError((_) => null);
  }

  void _next() {
    _i = (_i + 1) % _urls.length;
    _play();
  }

  void _fadeTo(double target, int ms) {
    _fade?.cancel();
    final el = _el;
    if (el == null) return;
    final start = el.volume;
    const step = 50;
    final steps = (ms / step).ceil();
    var n = 0;
    _fade = Timer.periodic(const Duration(milliseconds: step), (t) {
      n++;
      final v = start + (target - start) * (n / steps);
      el.volume = v.clamp(0.0, 1.0);
      if (n >= steps) t.cancel();
    });
  }

  void setMuted(bool m) {
    _muted = m;
    _fadeTo(m ? 0 : _volume, 400);
  }

  void pause() => _el?.pause();

  void resume() => _el?.play().toDart.catchError((_) => null);

  void dispose() {
    _fade?.cancel();
    final el = _el;
    if (el != null) {
      el.pause();
      el.removeAttribute('src');
      el.load();
    }
    _el = null;
  }
}

/// Comparte el fichero (Web Share API) si el navegador puede; si no, lo descarga. Devuelve true si se compartió.
Future<bool> shareOrDownload(Uint8List bytes, String filename, String mime, {bool share = true}) async {
  final blob = web.Blob([bytes.toJS].toJS, web.BlobPropertyBag(type: mime));
  if (share) {
    var can = false;
    try {
      final file = web.File([bytes.toJS].toJS, filename, web.FilePropertyBag(type: mime));
      final data = web.ShareData(files: [file].toJS);
      final nav = web.window.navigator;
      can = nav.canShare(data);
      if (can) await nav.share(data).toDart;
    } catch (_) {
      // Sin Web Share, o el usuario lo ha cancelado.
    }
    if (can) return true;
  }
  final url = web.URL.createObjectURL(blob);
  final a = web.HTMLAnchorElement()
    ..href = url
    ..download = filename;
  web.document.body?.append(a);
  a.click();
  a.remove();
  Timer(const Duration(seconds: 30), () => web.URL.revokeObjectURL(url));
  return false;
}
