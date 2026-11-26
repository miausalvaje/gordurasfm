import 'package:material_ui/material_ui.dart';

import 'api.dart';

/// Sesión del usuario: avisa al router cuando entra o sale.
class Session extends ChangeNotifier {
  Session(this.api) {
    api.onUnauthorized = clear;
  }

  final Api api;
  User? user;

  bool get loggedIn => user != null;

  Future<void> load() async {
    try {
      user = await api.me();
    } catch (_) {
      user = null;
    }
    notifyListeners();
  }

  void set(User u) {
    user = u;
    notifyListeners();
  }

  void clear() {
    if (user == null) return;
    user = null;
    notifyListeners();
  }

  Future<void> logout() async {
    try {
      await api.logout();
    } catch (_) {}
    user = null;
    notifyListeners();
  }
}

/// Da acceso a la API y a la sesión desde cualquier widget.
class AppScope extends InheritedWidget {
  const AppScope({super.key, required this.api, required this.session, required super.child});

  final Api api;
  final Session session;

  static AppScope of(BuildContext context) => context.dependOnInheritedWidgetOfExactType<AppScope>()!;

  @override
  bool updateShouldNotify(AppScope old) => old.api != api || old.session != session;
}

extension ScopeX on BuildContext {
  Api get api => AppScope.of(this).api;
  Session get session => AppScope.of(this).session;
}
