// Funciones del navegador. En los tests (VM) se usa la versión vacía.
export 'browser_stub.dart' if (dart.library.js_interop) 'browser_web.dart';
