import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../l10n/es.dart';
import '../l10n/wrapped_es.dart';
import '../platform/browser.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';
import '../widgets/cover.dart';
import '../widgets/top_items.dart';

const zonasComunes = [
  'Europe/Madrid', 'Atlantic/Canary', 'Europe/Lisbon', 'Europe/London', 'Europe/Paris', 'Europe/Berlin', //
  'Europe/Rome', 'America/Mexico_City', 'America/Bogota', 'America/Lima', 'America/Santiago',
  'America/Argentina/Buenos_Aires', 'America/Caracas', 'America/New_York', 'America/Los_Angeles', 'UTC',
];

class ProfileScreen extends StatefulWidget {
  const ProfileScreen({super.key});

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  Future<(StatsData, HistoryPage)>? _future;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _future ??= _load();
  }

  Future<(StatsData, HistoryPage)> _load() async {
    final api = context.api;
    final r = await Future.wait([api.stats('all_time', limit: 5), api.scrobbles(limit: 5)]);
    return (r[0] as StatsData, r[1] as HistoryPage);
  }

  Future<void> _edit(User u) async {
    final changed = await showDialog<User>(
      context: context,
      builder: (_) => _EditProfileDialog(user: u),
    );
    if (changed != null && mounted) {
      context.session.set(changed);
      setState(() => _future = _load());
      toast(context, T.guardado);
    }
  }

  @override
  Widget build(BuildContext context) {
    final u = context.session.user!;
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
      children: [
        ContentWidth(
          max: 900,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _Header(user: u, onEdit: () => _edit(u)),
              const SizedBox(height: 14),
              FutureBuilder(
                future: _future,
                builder: (context, snap) {
                  if (snap.hasError) {
                    return ErrorBox(error: snap.error!, onRetry: () => setState(() => _future = _load()));
                  }
                  if (!snap.hasData) return const Loading();
                  final (stats, recent) = snap.data!;
                  return _Body(stats: stats, recent: recent);
                },
              ),
              const SizedBox(height: 24),
              _Links(user: u),
            ],
          ),
        ),
      ],
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({required this.user, required this.onEdit});
  final User user;
  final VoidCallback onEdit;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(20),
    decoration: BoxDecoration(
      borderRadius: BorderRadius.circular(22),
      gradient: LinearGradient(
        colors: [C.accent.withValues(alpha: 0.35), C.accent2.withValues(alpha: 0.18), C.surface],
        begin: Alignment.topLeft,
        end: Alignment.bottomRight,
      ),
    ),
    child: Row(
      children: [
        Container(
          padding: const EdgeInsets.all(3),
          decoration: const BoxDecoration(gradient: C.gradient, shape: BoxShape.circle),
          child: ArtistAvatar(name: user.displayName, size: 76),
        ),
        const SizedBox(width: 16),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                user.displayName,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontFamily: 'InterDisplay', fontSize: 26, fontWeight: FontWeight.w900),
              ),
              Text('@${user.username}', style: const TextStyle(color: C.muted)),
              const SizedBox(height: 6),
              Text(
                T.miembroDesde(fmtLongDate(fromUnix(user.createdAt))),
                style: const TextStyle(fontSize: 12.5, color: C.muted),
              ),
            ],
          ),
        ),
        IconButton(tooltip: T.editarPerfil, onPressed: onEdit, icon: const Icon(Icons.edit_rounded)),
      ],
    ),
  );
}

class _Body extends StatelessWidget {
  const _Body({required this.stats, required this.recent});
  final StatsData stats;
  final HistoryPage recent;

  @override
  Widget build(BuildContext context) {
    final t = stats.totals;
    final nums = [
      (fmtInt(t.plays), T.escuchasMayus),
      (fmtDuration(t.listeningMs), T.tiempoEscuchado),
      (fmtInt(t.artists), T.artistas),
      (fmtInt(t.albums), T.albumes),
      (fmtInt(t.tracks), T.canciones),
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Panel(
          title: T.enTotal,
          child: LayoutBuilder(
            builder: (context, c) {
              final cols = c.maxWidth >= 600 ? 5 : 3;
              final w = (c.maxWidth - 12 * (cols - 1)) / cols;
              return Wrap(
                spacing: 12,
                runSpacing: 14,
                children: [
                  for (final (v, l) in nums)
                    SizedBox(
                      width: w,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            v,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: const TextStyle(fontSize: 21, fontWeight: FontWeight.w800),
                          ),
                          Text(l, maxLines: 2, style: const TextStyle(color: C.muted, fontSize: 12.5, height: 1.2)),
                        ],
                      ),
                    ),
                ],
              );
            },
          ),
        ),
        const SizedBox(height: 14),
        // Wrapped
        Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: BorderRadius.circular(18),
            onTap: () => context.go('/wrapped'),
            child: Ink(
              padding: const EdgeInsets.all(18),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(18),
                gradient: const LinearGradient(colors: [Color(0xFF3A1D5C), Color(0xFF7A1F4B)]),
              ),
              child: const Row(
                children: [
                  Icon(Icons.auto_awesome_rounded, size: 32, color: Colors.white),
                  SizedBox(width: 14),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          T.wrapped,
                          style: TextStyle(fontFamily: 'InterDisplay', fontSize: 20, fontWeight: FontWeight.w900),
                        ),
                        Text(TW.explica, style: TextStyle(color: Colors.white70, fontSize: 13)),
                      ],
                    ),
                  ),
                  Icon(Icons.chevron_right_rounded, color: Colors.white70),
                ],
              ),
            ),
          ),
        ),
        if (stats.topArtists.isNotEmpty) ...[
          SectionHeader(
            title: T.topArtists,
            action: T.verTodos,
            onAction: () => context.go('/top/artists?periodo=all_time'),
          ),
          SizedBox(
            height: 150,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              clipBehavior: Clip.none,
              itemCount: stats.topArtists.length,
              separatorBuilder: (_, _) => const SizedBox(width: 14),
              itemBuilder: (_, i) => ArtistBubble(item: stats.topArtists[i], size: 100, index: i),
            ),
          ),
        ],
        if (stats.topAlbums.isNotEmpty) ...[
          SectionHeader(
            title: T.topAlbums,
            action: T.verTodos,
            onAction: () => context.go('/top/albums?periodo=all_time'),
          ),
          SizedBox(
            height: 200,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              clipBehavior: Clip.none,
              itemCount: stats.topAlbums.length,
              separatorBuilder: (_, _) => const SizedBox(width: 14),
              itemBuilder: (_, i) => AlbumCard(item: stats.topAlbums[i], width: 130, index: i),
            ),
          ),
        ],
        if (stats.topTracks.isNotEmpty) ...[
          SectionHeader(
            title: T.topTracks,
            action: T.verTodos,
            onAction: () => context.go('/top/tracks?periodo=all_time'),
          ),
          Panel(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: Column(
              children: [
                for (final (i, it) in stats.topTracks.indexed)
                  TrackRow(item: it, maxPlays: stats.topTracks.first.plays, index: i),
              ],
            ),
          ),
        ],
        if (recent.items.isNotEmpty) ...[
          SectionHeader(title: T.recientes, action: T.verHistorial, onAction: () => context.go('/historial')),
          Panel(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            child: Column(
              children: [
                for (final s in recent.items)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 6),
                    child: Row(
                      children: [
                        Cover(url: s.cover, seed: s.album ?? s.track, size: 42),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                s.track,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(fontWeight: FontWeight.w600),
                              ),
                              Text(
                                s.artist,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(color: C.muted, fontSize: 13),
                              ),
                            ],
                          ),
                        ),
                        Text(fmtRelative(fromUnix(s.playedAt)), style: const TextStyle(color: C.muted, fontSize: 12.5)),
                      ],
                    ),
                  ),
              ],
            ),
          ),
        ],
      ],
    );
  }
}

class _Links extends StatelessWidget {
  const _Links({required this.user});
  final User user;

  @override
  Widget build(BuildContext context) => Card(
    child: Column(
      children: [
        ListTile(
          leading: const Icon(Icons.link_rounded),
          title: const Text(T.navAjustes),
          trailing: const Icon(Icons.chevron_right_rounded),
          onTap: () => context.go('/ajustes/conexiones'),
        ),
        if (user.isAdmin) ...[
          const Divider(),
          ListTile(
            leading: const Icon(Icons.admin_panel_settings_outlined),
            title: const Text(T.navAdmin),
            trailing: const Icon(Icons.chevron_right_rounded),
            onTap: () => context.go('/admin'),
          ),
        ],
        const Divider(),
        ListTile(
          leading: const Icon(Icons.logout_rounded, color: C.bad),
          title: const Text(T.salir, style: TextStyle(color: C.bad)),
          onTap: () => context.session.logout(),
        ),
      ],
    ),
  );
}

class _EditProfileDialog extends StatefulWidget {
  const _EditProfileDialog({required this.user});
  final User user;

  @override
  State<_EditProfileDialog> createState() => _EditProfileDialogState();
}

class _EditProfileDialogState extends State<_EditProfileDialog> {
  late final _name = TextEditingController(text: widget.user.displayName);
  late String _tz = widget.user.timezone;
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final u = await context.api.updateMe({'display_name': _name.text.trim(), 'timezone': _tz});
      if (mounted) Navigator.of(context).pop(u);
    } on ApiException catch (e) {
      setState(() => _error = e.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final browserTz = browserTimeZone();
    final zones = {...zonasComunes, _tz, browserTz}.toList();
    return AlertDialog(
      title: const Text(T.editarPerfil),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            TextField(
              controller: _name,
              decoration: const InputDecoration(labelText: T.nombreVisible),
              maxLength: 60,
            ),
            const SizedBox(height: 8),
            DropdownButtonFormField<String>(
              key: ValueKey(_tz),
              initialValue: _tz,
              isExpanded: true,
              decoration: const InputDecoration(labelText: T.zonaHoraria),
              items: [for (final z in zones) DropdownMenuItem(value: z, child: Text(z))],
              onChanged: (v) => setState(() => _tz = v ?? _tz),
            ),
            if (browserTz != _tz)
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton(
                  onPressed: () => setState(() => _tz = browserTz),
                  child: Text(T.usarZonaNavegador(browserTz)),
                ),
              ),
            const SizedBox(height: 4),
            const Text(T.zonaExplica, style: TextStyle(color: C.muted, fontSize: 12.5)),
            if (_error != null) ...[const SizedBox(height: 10), Text(_error!, style: const TextStyle(color: C.bad))],
          ],
        ),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.of(context).pop(), child: const Text(T.cancelar)),
        FilledButton(onPressed: _busy ? null : _save, child: const Text(T.guardar)),
      ],
    );
  }
}
