import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../l10n/es.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';
import '../widgets/cover.dart';

/// Historial de escuchas, agrupado por día y con carga al bajar.
class HistoryScreen extends StatefulWidget {
  const HistoryScreen({super.key});

  @override
  State<HistoryScreen> createState() => _HistoryScreenState();
}

class _HistoryScreenState extends State<HistoryScreen> {
  final _items = <Scrobble>[];
  int _total = 0;
  int? _next;
  bool _started = false;
  bool _loading = false;
  bool _done = false;
  Object? _error;
  final _scroll = ScrollController();

  @override
  void initState() {
    super.initState();
    _scroll.addListener(() {
      if (_scroll.position.pixels > _scroll.position.maxScrollExtent - 600) _more();
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_started) {
      _started = true;
      _more();
    }
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  Future<void> _more() async {
    if (_loading || _done) return;
    setState(() => _loading = true);
    try {
      final p = await context.api.scrobbles(before: _next);
      if (!mounted) return;
      setState(() {
        _items.addAll(p.items);
        _total = p.total;
        _next = p.nextBefore;
        _done = p.nextBefore == null;
      });
    } catch (e) {
      if (mounted) setState(() => _error = e);
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  String _dayLabel(DateTime d) {
    final now = DateTime.now();
    final today = DateTime(now.year, now.month, now.day);
    final day = DateTime(d.year, d.month, d.day);
    final diff = today.difference(day).inDays;
    if (diff == 0) return T.hoy;
    if (diff == 1) return T.ayer;
    final s = '${diasSemana[d.weekday - 1]} ${d.day} de ${meses[d.month - 1]}';
    return d.year == now.year ? s : '$s de ${d.year}';
  }

  @override
  Widget build(BuildContext context) {
    // Filas con cabecera de día intercalada.
    final rows = <Object>[];
    DateTime? lastDay;
    for (final s in _items) {
      final d = fromUnix(s.playedAt);
      final day = DateTime(d.year, d.month, d.day);
      if (lastDay != day) {
        rows.add(_dayLabel(d));
        lastDay = day;
      }
      rows.add(s);
    }
    return ListView.builder(
      controller: _scroll,
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
      itemCount: rows.length + 2,
      itemBuilder: (_, i) {
        if (i == 0) {
          return ContentWidth(
            max: 820,
            child: Padding(
              padding: const EdgeInsets.only(bottom: 4),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    T.navHistorial,
                    style: TextStyle(fontFamily: 'InterDisplay', fontSize: 28, fontWeight: FontWeight.w900),
                  ),
                  if (_items.isNotEmpty || _done)
                    Text(T.historialTotal(fmtInt(_total)), style: const TextStyle(color: C.muted)),
                ],
              ),
            ),
          );
        }
        if (i == rows.length + 1) {
          if (_error != null) return ErrorBox(error: _error!, onRetry: () => setState(() => _error = null));
          if (_loading) return const Loading();
          if (_done && _items.isEmpty) {
            return const Padding(
              padding: EdgeInsets.all(32),
              child: Text(
                T.vacioTitulo,
                textAlign: TextAlign.center,
                style: TextStyle(color: C.muted),
              ),
            );
          }
          return const SizedBox(height: 24);
        }
        final r = rows[i - 1];
        if (r is String) {
          return ContentWidth(
            max: 820,
            child: Padding(
              padding: const EdgeInsets.only(top: 22, bottom: 6),
              child: Text(
                r[0].toUpperCase() + r.substring(1),
                style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 15, color: C.accent2),
              ),
            ),
          );
        }
        return ContentWidth(max: 820, child: _ScrobbleRow(s: r as Scrobble));
      },
    );
  }
}

class _ScrobbleRow extends StatelessWidget {
  const _ScrobbleRow({required this.s});
  final Scrobble s;

  @override
  Widget build(BuildContext context) {
    final d = fromUnix(s.playedAt);
    final src = s.source.startsWith('navidrome') ? null : T.fuente[s.source];
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          Cover(url: s.cover, seed: s.album ?? s.track, size: 46),
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
                const SizedBox(height: 2),
                Text(
                  [s.artist, s.album].whereType<String>().join(' · '),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: C.muted, fontSize: 13),
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Text(
                fmtTime(d),
                style: const TextStyle(color: C.muted, fontFeatures: [FontFeature.tabularFigures()]),
              ),
              if (src != null)
                Container(
                  margin: const EdgeInsets.only(top: 4),
                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
                  decoration: BoxDecoration(color: C.surface2, borderRadius: BorderRadius.circular(6)),
                  child: Text(src, style: const TextStyle(fontSize: 10.5, color: C.muted)),
                ),
            ],
          ),
        ],
      ),
    );
  }
}
