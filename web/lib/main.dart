import 'package:flutter/semantics.dart' show SemanticsBinding;
import 'package:flutter_web_plugins/url_strategy.dart';
import 'package:material_ui/material_ui.dart';

import 'api.dart';
import 'app.dart';
import 'platform/browser.dart';
import 'session.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  usePathUrlStrategy();
  // En web Flutter pinta en canvas y solo crea el <input> al enfocar un campo,
  // por lo que Bitwarden y otros gestores no ven el formulario. Con la
  // semántica activa los campos existen siempre en el DOM.
  SemanticsBinding.instance.ensureSemantics();
  final api = Api();
  final session = Session(api);
  await session.load();
  runApp(GordurasApp(api: api, session: session));
  removeSplash();
}
