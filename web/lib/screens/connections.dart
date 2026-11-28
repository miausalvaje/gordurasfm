import 'dart:async';
import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../l10n/es.dart';
import '../platform/browser.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';

/// Conexiones (Navidrome, ListenBrainz, Last.fm), reenvíos, importar y exportar.
class ConnectionsScreen extends StatefulWidget {
  const ConnectionsScreen({super.key, this.lastfmResult});

  /// `ok` o `error` al volver de autorizar en Last.fm.
  final String? lastfmResult;

  @override
  State<ConnectionsScreen> createState() => _ConnectionsScreenState();
}

class _ConnectionsScreenState extends State<ConnectionsScreen> {
  List<dynamic> _tokens = [];
  List<dynamic> _conns = [];
  List<dynamic> _queue = [];
  List<dynamic> _jobs = [];
  Map<String, dynamic> _lfm = {};
  Map<String, dynamic> _sync = {};
  String? _newToken;
  bool _loaded = false;
  Object? _error;
  Timer? _poll;
  final _lbToken = TextEditingController();
  final _lfmUser = TextEditingController();
  final _ownKey = TextEditingController();
  final _ownSecret = TextEditingController();
  bool _busy = false;

  Api get api => context.api;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_loaded && _error == null) {
      _load();
      final r = widget.lastfmResult;
      if (r != null) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) toast(context, r == 'ok' ? T.lfmOk : T.lfmError, error: r != 'ok');
        });
      }
    }
  }

  @override
  void dispose() {
    _poll?.cancel();
    for (final c in [_lbToken, _lfmUser, _ownKey, _ownSecret]) {
      c.dispose();
    }
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final r = await Future.wait([
        api.get('/api/v1/tokens'),
        api.get('/api/v1/connections'),
        api.get('/api/v1/forward/status'),
        api.get('/api/v1/import/jobs'),
        api.get('/api/v1/lastfm/status'),
        api.get('/api/v1/import/lastfm/sync'),
      ]);
      if (!mounted) return;
      setState(() {
        _tokens = r[0] as List;
        _conns = r[1] as List;
        _queue = r[2] as List;
        _jobs = r[3] as List;
        _lfm = r[4] as Map<String, dynamic>;
        _sync = r[5] as Map<String, dynamic>;
        _loaded = true;
        _error = null;
      });
      _schedulePoll();
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  bool get _jobRunning => _jobs.any((j) => j['status'] == 'queued' || j['status'] == 'running');

  void _schedulePoll() {
    _poll?.cancel();
    if (_jobRunning) _poll = Timer(const Duration(seconds: 2), _load);
  }

  Map<String, dynamic>? _conn(String p) =>
      _conns.cast<Map<String, dynamic>?>().firstWhere((c) => c!['provider'] == p, orElse: () => null);

  /// Ejecuta una acción con aviso de error y recarga.
  Future<void> _run(Future<void> Function() f, {String? ok}) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      await f();
      if (ok != null && mounted) toast(context, ok);
      await _load();
    } on ApiException catch (e) {
      if (mounted) toast(context, e.message, error: true);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (!_loaded) {
      return _error != null ? ErrorBox(error: _error!, onRetry: _load) : const Loading();
    }
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 40),
      children: [
        ContentWidth(
          max: 820,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text(
                T.conexiones,
                style: TextStyle(fontFamily: 'InterDisplay', fontSize: 28, fontWeight: FontWeight.w900),
              ),
              const SizedBox(height: 14),
              _navidrome(),
              const SizedBox(height: 14),
              _listenbrainz(),
              const SizedBox(height: 14),
              _lastfm(),
              if (_queue.isNotEmpty) ...[const SizedBox(height: 14), _queuePanel()],
              const SizedBox(height: 14),
              _import(),
              const SizedBox(height: 14),
              Panel(
                title: T.exportar,
                child: Wrap(
                  spacing: 10,
                  runSpacing: 10,
                  children: [
                    OutlinedButton.icon(
                      onPressed: () => download('/api/v1/export?format=csv'),
                      icon: const Icon(Icons.download_rounded),
                      label: const Text('CSV'),
                    ),
                    OutlinedButton.icon(
                      onPressed: () => download('/api/v1/export?format=json'),
                      icon: const Icon(Icons.download_rounded),
                      label: const Text('JSON'),
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

  // ---------- Navidrome ----------

  Widget _navidrome() => Panel(
    title: T.navidromeTitulo,
    subtitle: T.navidromeExplica,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (_newToken != null) ...[
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: C.accent.withValues(alpha: 0.1),
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: C.accent.withValues(alpha: 0.4)),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                const Text(T.tokenNuevo, style: TextStyle(fontWeight: FontWeight.w600)),
                const SizedBox(height: 8),
                SelectableText(_newToken!, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600)),
                const SizedBox(height: 8),
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton.tonalIcon(
                    onPressed: () async {
                      await Clipboard.setData(ClipboardData(text: _newToken!));
                      if (mounted) toast(context, T.copiado);
                    },
                    icon: const Icon(Icons.copy_rounded, size: 18),
                    label: const Text(T.copiar),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 12),
        ],
        for (final t in _tokens.where((t) => t['kind'] == 'scrobble'))
          ListTile(
            contentPadding: EdgeInsets.zero,
            leading: const Icon(Icons.key_rounded, color: C.accent2),
            title: Text(T.tokenHint(t['hint'] as String)),
            subtitle: Text(
              t['last_used_at'] == null ? T.nuncaUsado : T.ultimoUso(fmtRelative(fromUnix(t['last_used_at'] as int))),
            ),
            trailing: IconButton(
              tooltip: T.borrar,
              icon: const Icon(Icons.delete_outline_rounded),
              onPressed: () => _run(() async => api.send('DELETE', '/api/v1/tokens/${t['id']}')),
            ),
          ),
        const SizedBox(height: 6),
        Align(
          alignment: Alignment.centerLeft,
          child: FilledButton.icon(
            onPressed: _busy
                ? null
                : () => _run(() async {
                    final r = await api.send('POST', '/api/v1/tokens', {'kind': 'scrobble', 'label': 'Navidrome'});
                    _newToken = r['token'] as String;
                  }),
            icon: const Icon(Icons.add_rounded),
            label: const Text(T.generarToken),
          ),
        ),
        const SizedBox(height: 12),
        const Text(T.navidromeAdmin, style: TextStyle(color: C.muted, fontSize: 12.5)),
        const SizedBox(height: 6),
        Container(
          padding: const EdgeInsets.fromLTRB(12, 6, 4, 6),
          decoration: BoxDecoration(color: C.surface2, borderRadius: BorderRadius.circular(10)),
          child: Row(
            children: [
              Expanded(
                child: SelectableText(
                  'ND_LISTENBRAINZ_BASEURL=${origin()}/1/',
                  style: const TextStyle(fontSize: 12.5, color: C.text, fontWeight: FontWeight.w600, height: 1.4),
                ),
              ),
              IconButton(
                tooltip: T.copiar,
                icon: const Icon(Icons.copy_rounded, size: 18),
                onPressed: () async {
                  await Clipboard.setData(ClipboardData(text: 'ND_LISTENBRAINZ_BASEURL=${origin()}/1/'));
                  if (mounted) toast(context, T.copiado);
                },
              ),
            ],
          ),
        ),
      ],
    ),
  );

  // ---------- ListenBrainz ----------

  Widget _listenbrainz() {
    final c = _conn('listenbrainz');
    return Panel(
      title: T.listenbrainz,
      subtitle: c == null ? T.lbExplica : null,
      child: c == null
          ? Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                TextField(
                  controller: _lbToken,
                  decoration: const InputDecoration(labelText: T.lbToken, isDense: true),
                ),
                const SizedBox(height: 10),
                Align(
                  alignment: Alignment.centerLeft,
                  child: FilledButton(
                    onPressed: _busy
                        ? null
                        : () => _run(() async {
                            await api.send('PUT', '/api/v1/connections/listenbrainz', {'token': _lbToken.text.trim()});
                            _lbToken.clear();
                          }),
                    child: const Text(T.conectar),
                  ),
                ),
              ],
            )
          : _connectedBlock(c, 'listenbrainz'),
    );
  }

  Widget _connectedBlock(Map<String, dynamic> c, String provider, {String? warning}) {
    final failed = c['status'] != 'ok';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Icon(
              failed ? Icons.error_outline_rounded : Icons.check_circle_rounded,
              color: failed ? C.bad : C.ok,
              size: 20,
            ),
            const SizedBox(width: 8),
            Expanded(child: Text(failed ? T.reconectar : T.conectadoComo('${c['remote_username'] ?? ''}'))),
          ],
        ),
        if (failed && c['last_error'] != null)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text('${c['last_error']}', style: const TextStyle(color: C.muted, fontSize: 12)),
          ),
        SwitchListTile(
          contentPadding: EdgeInsets.zero,
          title: const Text(T.reenviar),
          subtitle: warning == null ? null : Text(warning, style: const TextStyle(fontSize: 12.5)),
          value: c['forward_enabled'] == true,
          onChanged: _busy
              ? null
              : (v) => _run(() async => api.send('PATCH', '/api/v1/connections/$provider', {'forward_enabled': v})),
        ),
        Align(
          alignment: Alignment.centerLeft,
          child: TextButton.icon(
            style: TextButton.styleFrom(foregroundColor: C.bad),
            onPressed: _busy ? null : () => _run(() async => api.send('DELETE', '/api/v1/connections/$provider')),
            icon: const Icon(Icons.link_off_rounded, size: 18),
            label: const Text(T.desconectar),
          ),
        ),
      ],
    );
  }

  // ---------- Last.fm ----------

  Widget _lastfm() {
    final c = _conn('lastfm');
    final available = _lfm['available'] == true;
    return Panel(
      title: T.lastfm,
      subtitle: c == null ? (available ? T.lfmExplica : T.lfmSinKey) : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (c != null)
            _connectedBlock(c, 'lastfm', warning: T.lfmAvisoDuplicado)
          else if (available)
            Align(
              alignment: Alignment.centerLeft,
              child: FilledButton.icon(
                onPressed: _busy
                    ? null
                    : () => _run(() async {
                        final r = await api.get('/api/v1/connections/lastfm/start');
                        redirectTo(r['url'] as String);
                      }),
                icon: const Icon(Icons.open_in_new_rounded, size: 18),
                label: const Text(T.lfmConectar),
              ),
            ),
          const SizedBox(height: 4),
          Theme(
            data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
            child: ExpansionTile(
              tilePadding: EdgeInsets.zero,
              title: Text(T.lfmPropiaKey + (_lfm['own_key'] == true ? ' ✓' : ''), style: const TextStyle(fontSize: 14)),
              childrenPadding: const EdgeInsets.only(bottom: 8),
              children: [
                TextField(
                  controller: _ownKey,
                  decoration: const InputDecoration(labelText: T.lfmKey, isDense: true),
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _ownSecret,
                  obscureText: true,
                  decoration: const InputDecoration(labelText: T.lfmSecret, isDense: true),
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    FilledButton.tonal(
                      onPressed: _busy
                          ? null
                          : () => _run(() async {
                              await api.send('PUT', '/api/v1/connections/lastfm/key', {
                                'api_key': _ownKey.text.trim(),
                                'api_secret': _ownSecret.text.trim(),
                              });
                              _ownKey.clear();
                              _ownSecret.clear();
                            }, ok: T.guardado),
                      child: const Text(T.guardar),
                    ),
                    if (_lfm['own_key'] == true) ...[
                      const SizedBox(width: 8),
                      TextButton(
                        onPressed: () => _run(() async => api.send('DELETE', '/api/v1/connections/lastfm/key')),
                        child: const Text(T.borrar),
                      ),
                    ],
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  // ---------- Cola de reenvíos ----------

  Widget _queuePanel() {
    final byTarget = <String, List<dynamic>>{};
    for (final q in _queue) {
      byTarget.putIfAbsent(q['target'] as String, () => []).add(q);
    }
    final retryable = _queue.any((q) => q['status'] == 'paused' || q['status'] == 'failed');
    return Panel(
      title: T.cola,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final e in byTarget.entries)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Text(
                    e.key == 'lastfm' ? T.lastfm : T.listenbrainz,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                  for (final q in e.value)
                    Chip(
                      visualDensity: VisualDensity.compact,
                      label: Text('${fmtInt(q['count'] as int)} ${T.estadosCola[q['status']] ?? q['status']}'),
                    ),
                ],
              ),
            ),
          if (retryable)
            Align(
              alignment: Alignment.centerLeft,
              child: OutlinedButton(
                onPressed: _busy ? null : () => _run(() async => api.send('POST', '/api/v1/forward/retry')),
                child: const Text(T.reintentar),
              ),
            ),
        ],
      ),
    );
  }

  // ---------- Importar ----------

  Widget _import() {
    final lfmConn = _conn('lastfm');
    final canLfm = _lfm['available'] == true;
    return Panel(
      title: T.importar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (canLfm) ...[
            if (lfmConn == null)
              Padding(
                padding: const EdgeInsets.only(bottom: 10),
                child: TextField(
                  controller: _lfmUser,
                  decoration: const InputDecoration(labelText: T.usuarioLfm, isDense: true),
                ),
              ),
            Wrap(
              spacing: 10,
              runSpacing: 10,
              children: [
                FilledButton.icon(
                  onPressed: _busy || _jobRunning ? null : () => _startLfm('full', lfmConn == null),
                  icon: const Icon(Icons.cloud_download_rounded, size: 18),
                  label: const Text(T.importarLfmTodo),
                ),
                OutlinedButton(
                  onPressed: _busy || _jobRunning ? null : () => _startLfm('new', lfmConn == null),
                  child: const Text(T.importarLfmNuevo),
                ),
              ],
            ),
            if ((_sync['interval_secs'] ?? 0) != 0) ...[
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text(T.sincronizarLfm),
                subtitle: Text(
                  _sync['enabled'] == true && _sync['last_error'] != null
                      ? '${T.sincronizarLfmError}: ${_sync['last_error']}'
                      : T.sincronizarLfmExplica,
                  style: const TextStyle(fontSize: 12.5),
                ),
                value: _sync['enabled'] == true,
                onChanged: _busy
                    ? null
                    : (v) => _run(() async {
                        if (v) {
                          await api.send('PUT', '/api/v1/import/lastfm/sync', {
                            if (lfmConn == null) 'username': _lfmUser.text.trim(),
                          });
                        } else {
                          await api.send('DELETE', '/api/v1/import/lastfm/sync');
                        }
                      }),
              ),
            ],
            const SizedBox(height: 18),
          ],
          Align(
            alignment: Alignment.centerLeft,
            child: OutlinedButton.icon(
              onPressed: _busy || _jobRunning ? null : _upload,
              icon: const Icon(Icons.upload_file_rounded, size: 18),
              label: const Text(T.importarFichero),
            ),
          ),
          const SizedBox(height: 6),
          const Text(T.importarFicheroExplica, style: TextStyle(color: C.muted, fontSize: 12.5)),
          for (final j in _jobs.take(5))
            _JobTile(
              job: j,
              busy: _busy,
              onAction: (a) => _run(() async {
                await api.send('POST', '/api/v1/import/jobs/${j['id']}/$a');
              }),
            ),
        ],
      ),
    );
  }

  void _startLfm(String mode, bool needsUser) => _run(() async {
    await api.send('POST', '/api/v1/import/lastfm', {'mode': mode, if (needsUser) 'username': _lfmUser.text.trim()});
  });

  Future<void> _upload() async {
    setState(() => _busy = true);
    try {
      final r = await pickAndUpload('/api/v1/import/file', '.csv,.json,.jsonl,.txt');
      if (r == null || !mounted) return;
      if (r.status >= 200 && r.status < 300) {
        final j = jsonDecode(r.body) as Map<String, dynamic>;
        toast(context, '${fmtInt(j['listens'] as int)} escuchas encontradas');
      } else {
        String msg = T.errorGenerico;
        try {
          msg = (jsonDecode(r.body) as Map)['message'] as String;
        } catch (_) {}
        toast(context, msg, error: true);
      }
    } catch (_) {
      if (mounted) toast(context, T.errorRed, error: true);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
    await _load();
  }
}

class _JobTile extends StatelessWidget {
  const _JobTile({required this.job, required this.busy, required this.onAction});
  final dynamic job;
  final bool busy;
  final void Function(String action) onAction;

  @override
  Widget build(BuildContext context) {
    final status = job['status'] as String;
    final done = (job['progress_done'] ?? 0) as int;
    final total = job['progress_total'] as int?;
    final stats = (job['stats'] ?? const {}) as Map;
    final running = status == 'queued' || status == 'running';
    final color = switch (status) {
      'done' => C.ok,
      'failed' => C.bad,
      _ => C.accent2,
    };
    return Container(
      margin: const EdgeInsets.only(top: 14),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(color: C.surface2, borderRadius: BorderRadius.circular(12)),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  T.trabajos[job['kind']] ?? '${job['kind']}',
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
              ),
              Text(
                T.estadosTrabajo[status] ?? status,
                style: TextStyle(color: color, fontWeight: FontWeight.w600, fontSize: 13),
              ),
            ],
          ),
          const SizedBox(height: 8),
          if (running || total != null)
            ClipRRect(
              borderRadius: BorderRadius.circular(4),
              child: LinearProgressIndicator(
                value: total == null || total == 0 ? null : (done / total).clamp(0.0, 1.0),
                minHeight: 6,
                backgroundColor: C.line,
                color: color,
              ),
            ),
          const SizedBox(height: 6),
          Text(
            '${total != null ? '${fmtInt(done)} / ${fmtInt(total)} · ' : ''}'
            '${T.resumenImport((stats['stored'] ?? 0) as int, (stats['duplicates'] ?? 0) as int, (stats['invalid'] ?? 0) as int)}',
            style: const TextStyle(color: C.muted, fontSize: 12.5),
          ),
          if (job['error'] != null) ...[
            const SizedBox(height: 4),
            Text('${job['error']}', style: const TextStyle(color: C.bad, fontSize: 12.5)),
          ],
          if (running || status == 'failed' || status == 'cancelled')
            Align(
              alignment: Alignment.centerRight,
              child: running
                  ? TextButton(onPressed: busy ? null : () => onAction('cancel'), child: const Text(T.cancelarTrabajo))
                  : TextButton(onPressed: busy ? null : () => onAction('resume'), child: const Text(T.reanudar)),
            ),
        ],
      ),
    );
  }
}
