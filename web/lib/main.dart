import 'package:flutter_web_plugins/url_strategy.dart';
import 'package:material_ui/material_ui.dart';

import 'api.dart';
import 'app.dart';
import 'platform/browser.dart';
import 'session.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  usePathUrlStrategy();
  final api = Api();
  final session = Session(api);
  await session.load();
  runApp(GordurasApp(api: api, session: session));
  removeSplash();
}
