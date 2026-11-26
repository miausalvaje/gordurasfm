import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../l10n/es.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';

/// Administración: key de Last.fm de la instancia y usuarios.
class AdminScreen extends StatefulWidget {
  const AdminScreen({super.key});

  @override
  State<AdminScreen> createState() => _AdminScreenState();
}

class _AdminScreenState extends State<AdminScreen> {
  Map<String, dynamic>? _lfm;
  List<dynamic> _users = [];
  Object? _error;
  bool _busy = false;
  final _key = TextEditingController();
  final _secret = TextEditingController();

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_lfm == null && _error == null) _load();
  }

  @override
  void dispose() {
    _key.dispose();
    _secret.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final r = await Future.wait([context.api.get('/api/v1/admin/lastfm'), context.api.get('/api/v1/admin/users')]);
      if (mounted) {
        setState(() {
          _lfm = r[0] as Map<String, dynamic>;
          _users = r[1] as List;
        });
      }
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _run(Future<void> Function() f) async {
    setState(() => _busy = true);
    try {
      await f();
      await _load();
      if (mounted) toast(context, T.guardado);
    } on ApiException catch (e) {
      if (mounted) toast(context, e.message, error: true);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final me = context.session.user!;
    if (!me.isAdmin) {
      WidgetsBinding.instance.addPostFrameCallback((_) => context.go('/perfil'));
      return const SizedBox.shrink();
    }
    if (_lfm == null) return _error != null ? ErrorBox(error: _error!, onRetry: _load) : const Loading();
    final lfm = _lfm!;
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 40),
      children: [
        ContentWidth(
          max: 820,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text(
                T.navAdmin,
                style: TextStyle(fontFamily: 'InterDisplay', fontSize: 28, fontWeight: FontWeight.w900),
              ),
              const SizedBox(height: 14),
              Panel(
                title: T.adminLfm,
                subtitle: T.adminLfmExplica,
                child: lfm['source'] == 'config'
                    ? Text('${T.adminLfmConfig} (${lfm['api_key']})', style: const TextStyle(color: C.muted))
                    : Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          if (lfm['configured'] == true)
                            Padding(
                              padding: const EdgeInsets.only(bottom: 10),
                              child: Row(
                                children: [
                                  const Icon(Icons.check_circle_rounded, color: C.ok, size: 20),
                                  const SizedBox(width: 8),
                                  Expanded(child: Text('${lfm['api_key']}')),
                                  TextButton(
                                    onPressed: _busy
                                        ? null
                                        : () => _run(() async => context.api.send('DELETE', '/api/v1/admin/lastfm')),
                                    child: const Text(T.borrar),
                                  ),
                                ],
                              ),
                            ),
                          TextField(
                            controller: _key,
                            decoration: const InputDecoration(labelText: T.lfmKey, isDense: true),
                          ),
                          const SizedBox(height: 8),
                          TextField(
                            controller: _secret,
                            obscureText: true,
                            decoration: const InputDecoration(labelText: T.lfmSecret, isDense: true),
                          ),
                          const SizedBox(height: 10),
                          Align(
                            alignment: Alignment.centerLeft,
                            child: FilledButton(
                              onPressed: _busy
                                  ? null
                                  : () => _run(() async {
                                      await context.api.send('PUT', '/api/v1/admin/lastfm', {
                                        'api_key': _key.text.trim(),
                                        'api_secret': _secret.text.trim(),
                                      });
                                      _key.clear();
                                      _secret.clear();
                                    }),
                              child: const Text(T.guardar),
                            ),
                          ),
                        ],
                      ),
              ),
              const SizedBox(height: 14),
              Panel(
                title: T.usuarios,
                padding: const EdgeInsets.fromLTRB(18, 18, 18, 8),
                child: Column(
                  children: [
                    for (final u in _users)
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 8),
                        child: Row(
                          children: [
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('${u['username']}', style: const TextStyle(fontWeight: FontWeight.w600)),
                                  Text(
                                    '${fmtInt(u['scrobbles'] as int)} ${T.escuchas}',
                                    style: const TextStyle(color: C.muted, fontSize: 12.5),
                                  ),
                                ],
                              ),
                            ),
                            _Toggle(
                              label: T.aprobado,
                              value: u['is_approved'] == true,
                              enabled: !_busy && u['id'] != me.id,
                              onChanged: (v) => _run(
                                () async =>
                                    context.api.send('PATCH', '/api/v1/admin/users/${u['id']}', {'is_approved': v}),
                              ),
                            ),
                            const SizedBox(width: 6),
                            _Toggle(
                              label: T.admin,
                              value: u['is_admin'] == true,
                              enabled: !_busy && u['id'] != me.id,
                              onChanged: (v) => _run(
                                () async =>
                                    context.api.send('PATCH', '/api/v1/admin/users/${u['id']}', {'is_admin': v}),
                              ),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Toggle extends StatelessWidget {
  const _Toggle({required this.label, required this.value, required this.enabled, required this.onChanged});
  final String label;
  final bool value;
  final bool enabled;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: MainAxisSize.min,
    children: [
      Switch(value: value, onChanged: enabled ? onChanged : null),
      Text(label, style: const TextStyle(fontSize: 11, color: C.muted)),
    ],
  );
}
