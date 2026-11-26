import 'dart:async';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:web/web.dart' as web;

/// Resultado de subir un fichero.
typedef UploadResult = ({int status, String body});

/// Quita el indicador de carga de index.html.
void removeSplash() => web.document.getElementById('carga')?.remove();

/// Zona horaria IANA del navegador (p. ej. «Europe/Madrid»).
String browserTimeZone() {
  try {
    final intl = globalContext['Intl'] as JSObject;
    final dtf = intl.callMethod<JSObject>('DateTimeFormat'.toJS);
    final opts = dtf.callMethod<JSObject>('resolvedOptions'.toJS);
    return (opts['timeZone'] as JSString).toDart;
  } catch (_) {
    return 'Europe/Madrid';
  }
}

String origin() => web.window.location.origin;

void redirectTo(String url) => web.window.location.href = url;

/// Descarga un fichero del propio servidor (la cookie de sesión va sola).
void download(String url) {
  final a = web.HTMLAnchorElement()
    ..href = url
    ..download = '';
  web.document.body?.append(a);
  a.click();
  a.remove();
}

/// Abre el selector de ficheros y sube el elegido (campo `file`) sin cargarlo en memoria.
Future<UploadResult?> pickAndUpload(String url, String accept) {
  final done = Completer<UploadResult?>();
  final input = web.HTMLInputElement()
    ..type = 'file'
    ..accept = accept;
  void finish(UploadResult? r, [Object? error]) {
    if (done.isCompleted) return;
    if (error != null) {
      done.completeError(error);
    } else {
      done.complete(r);
    }
  }

  Future<void> upload(web.File f) async {
    try {
      final fd = web.FormData()..append('file', f, f.name);
      final headers = web.Headers()..append('X-Requested-With', 'GordurasFM');
      final r = await web.window
          .fetch(url.toJS, web.RequestInit(method: 'POST', body: fd, headers: headers, credentials: 'same-origin'))
          .toDart;
      final text = (await r.text().toDart).toDart;
      finish((status: r.status, body: text));
    } catch (e) {
      finish(null, e);
    }
  }

  input.addEventListener(
    'change',
    ((web.Event _) {
      final files = input.files;
      if (files == null || files.length == 0) {
        finish(null);
      } else {
        upload(files.item(0)!);
      }
    }).toJS,
  );
  input.addEventListener('cancel', ((web.Event _) => finish(null)).toJS);
  input.click();
  return done.future;
}
