import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../l10n/es.dart';
import '../session.dart';
import '../theme.dart';
import 'common.dart';

class _Dest {
  const _Dest(this.path, this.label, this.icon, this.selectedIcon);
  final String path;
  final String label;
  final IconData icon;
  final IconData selectedIcon;
}

const _dests = [
  _Dest('/', T.navInicio, Icons.insights_outlined, Icons.insights_rounded),
  _Dest('/top/albums', T.navTops, Icons.leaderboard_outlined, Icons.leaderboard_rounded),
  _Dest('/historial', T.navHistorial, Icons.history_rounded, Icons.history_rounded),
  _Dest('/perfil', T.navPerfil, Icons.person_outline_rounded, Icons.person_rounded),
];

int _indexFor(String path) {
  if (path.startsWith('/top')) return 1;
  if (path.startsWith('/historial')) return 2;
  if (path.startsWith('/perfil') || path.startsWith('/ajustes') || path.startsWith('/admin')) return 3;
  return 0;
}

/// Estructura común: barra inferior en móvil, raíl lateral en pantallas anchas.
class AppShell extends StatelessWidget {
  const AppShell({super.key, required this.location, required this.child});
  final String location;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 900;
    final idx = _indexFor(location);
    void go(int i) => context.go(_dests[i].path);
    final user = context.session.user;

    final appBar = AppBar(
      title: const Logo(),
      toolbarHeight: 60,
      actions: [
        IconButton(
          tooltip: T.navAjustes,
          icon: const Icon(Icons.tune_rounded),
          onPressed: () => context.go('/ajustes/conexiones'),
        ),
        const SizedBox(width: 4),
      ],
    );

    if (!wide) {
      return Scaffold(
        appBar: appBar,
        body: child,
        bottomNavigationBar: NavigationBar(
          selectedIndex: idx,
          onDestinationSelected: go,
          height: 66,
          destinations: [
            for (final d in _dests)
              NavigationDestination(icon: Icon(d.icon), selectedIcon: Icon(d.selectedIcon), label: d.label),
          ],
        ),
      );
    }

    return Scaffold(
      body: Row(
        children: [
          NavigationRail(
            selectedIndex: idx,
            onDestinationSelected: go,
            labelType: NavigationRailLabelType.all,
            leading: Padding(
              padding: const EdgeInsets.fromLTRB(8, 20, 8, 24),
              child: Container(
                width: 40,
                height: 40,
                decoration: const BoxDecoration(gradient: C.gradient, shape: BoxShape.circle),
                child: const Icon(Icons.graphic_eq_rounded, color: Colors.white),
              ),
            ),
            trailing: Expanded(
              child: Align(
                alignment: Alignment.bottomCenter,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 16),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      IconButton(
                        tooltip: T.navAjustes,
                        icon: const Icon(Icons.tune_rounded),
                        onPressed: () => context.go('/ajustes/conexiones'),
                      ),
                      if (user != null)
                        Padding(
                          padding: const EdgeInsets.only(top: 8),
                          child: Text(user.displayName, style: const TextStyle(color: C.muted, fontSize: 12)),
                        ),
                    ],
                  ),
                ),
              ),
            ),
            destinations: [
              for (final d in _dests)
                NavigationRailDestination(icon: Icon(d.icon), selectedIcon: Icon(d.selectedIcon), label: Text(d.label)),
            ],
          ),
          const VerticalDivider(width: 1),
          Expanded(
            child: Padding(padding: const EdgeInsets.only(top: 20), child: child),
          ),
        ],
      ),
    );
  }
}
