import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import 'api.dart';
import 'screens/admin.dart';
import 'screens/connections.dart';
import 'screens/history.dart';
import 'screens/home.dart';
import 'screens/login.dart';
import 'screens/profile.dart';
import 'screens/top.dart';
import 'screens/wrapped_list.dart';
import 'session.dart';
import 'theme.dart';
import 'widgets/shell.dart';
import 'wrapped/player.dart';

String periodOf(GoRouterState s) {
  final p = s.uri.queryParameters['periodo'];
  return p != null &&
          const {
            'this_week', 'this_month', 'this_year', 'last_week', 'last_month', 'last_year', 'all_time', //
          }.contains(p)
      ? p
      : 'this_week';
}

GoRouter buildRouter(Session session) => GoRouter(
  refreshListenable: session,
  redirect: (context, state) {
    final atLogin = state.matchedLocation == '/login';
    if (!session.loggedIn) {
      if (atLogin) return null;
      final from = state.uri.toString();
      return from == '/' ? '/login' : '/login?desde=${Uri.encodeComponent(from)}';
    }
    if (atLogin) return state.uri.queryParameters['desde'] ?? '/';
    return null;
  },
  routes: [
    GoRoute(path: '/login', builder: (_, _) => const LoginScreen()),
    // El reproductor del Wrapped va a pantalla completa, fuera del marco con la navegación.
    GoRoute(
      path: '/wrapped/:year',
      pageBuilder: (_, s) {
        final y = int.tryParse(s.pathParameters['year'] ?? '') ?? DateTime.now().year;
        return NoTransitionPage(
          key: ValueKey('wrapped$y'),
          child: WrappedPlayerScreen(year: y),
        );
      },
    ),
    ShellRoute(
      builder: (_, state, child) => AppShell(location: state.uri.path, child: child),
      routes: [
        GoRoute(
          path: '/',
          pageBuilder: (_, s) => NoTransitionPage(child: HomeScreen(period: periodOf(s))),
        ),
        GoRoute(
          path: '/top/:kind',
          pageBuilder: (_, s) {
            final kind = s.pathParameters['kind']!;
            return NoTransitionPage(
              child: TopScreen(
                kind: const {'albums', 'tracks', 'artists'}.contains(kind) ? kind : 'albums',
                period: periodOf(s),
              ),
            );
          },
        ),
        GoRoute(
          path: '/historial',
          pageBuilder: (_, _) => const NoTransitionPage(child: HistoryScreen()),
        ),
        GoRoute(
          path: '/wrapped',
          pageBuilder: (_, _) => const NoTransitionPage(child: WrappedListScreen()),
        ),
        GoRoute(
          path: '/perfil',
          pageBuilder: (_, _) => const NoTransitionPage(child: ProfileScreen()),
        ),
        GoRoute(
          path: '/ajustes/conexiones',
          pageBuilder: (_, s) =>
              NoTransitionPage(child: ConnectionsScreen(lastfmResult: s.uri.queryParameters['lastfm'])),
        ),
        GoRoute(
          path: '/admin',
          pageBuilder: (_, _) => const NoTransitionPage(child: AdminScreen()),
        ),
      ],
    ),
  ],
);

class GordurasApp extends StatefulWidget {
  const GordurasApp({super.key, required this.api, required this.session});
  final Api api;
  final Session session;

  @override
  State<GordurasApp> createState() => _GordurasAppState();
}

class _GordurasAppState extends State<GordurasApp> {
  late final GoRouter _router = buildRouter(widget.session);

  @override
  void dispose() {
    _router.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AppScope(
    api: widget.api,
    session: widget.session,
    child: MaterialApp.router(
      title: 'GordurasFM',
      debugShowCheckedModeBanner: false,
      theme: buildTheme(),
      routerConfig: _router,
    ),
  );
}
