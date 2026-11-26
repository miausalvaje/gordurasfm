import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../l10n/es.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';
import '../widgets/top_items.dart';

/// Top completo de álbumes, canciones o artistas, con carga por páginas al bajar.
class TopScreen extends StatefulWidget {
  const TopScreen({super.key, required this.kind, required this.period});
  final String kind;
  final String period;

  @override
  State<TopScreen> createState() => _TopScreenState();
}

class _TopScreenState extends State<TopScreen> {
  final _items = <TopItem>[];
  int _total = 0;
  int _page = 0;
  bool _loading = false;
  bool _done = false;
  Object? _error;
  int _req = 0;
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
    if (_page == 0 && !_loading) _more();
  }

  @override
  void didUpdateWidget(TopScreen old) {
    super.didUpdateWidget(old);
    if (old.kind != widget.kind || old.period != widget.period) {
      _req++;
      setState(() {
        _items.clear();
        _page = 0;
        _done = false;
        _loading = false;
        _error = null;
      });
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
    final req = _req;
    setState(() => _loading = true);
    try {
      final p = await context.api.top(widget.kind, widget.period, page: _page + 1);
      if (!mounted || req != _req) return;
      setState(() {
        _items.addAll(p.items);
        _total = p.total;
        _page = p.page;
        _done = p.items.length < 50 || _items.length >= p.total;
      });
    } catch (e) {
      if (mounted && req == _req) setState(() => _error = e);
    } finally {
      if (mounted && req == _req) setState(() => _loading = false);
    }
  }

  void _go({String? kind, String? period}) {
    final p = period ?? widget.period;
    context.go('/top/${kind ?? widget.kind}${p == 'this_week' ? '' : '?periodo=$p'}');
  }

  @override
  Widget build(BuildContext context) {
    final maxPlays = _items.isEmpty ? 1 : _items.first.plays;
    final wide = MediaQuery.sizeOf(context).width >= 700;
    return CustomScrollView(
      controller: _scroll,
      slivers: [
        SliverToBoxAdapter(
          child: ContentWidth(
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SegmentedButton<String>(
                    segments: [for (final e in T.tipoTop.entries) ButtonSegment(value: e.key, label: Text(e.value))],
                    selected: {widget.kind},
                    showSelectedIcon: false,
                    onSelectionChanged: (s) => _go(kind: s.first),
                  ),
                  const SizedBox(height: 14),
                  PeriodChips(
                    value: widget.period,
                    onChanged: (p) => _go(period: p),
                  ),
                  const SizedBox(height: 10),
                  if (_page > 0) Text(T.total(_total, widget.kind), style: const TextStyle(color: C.muted)),
                ],
              ),
            ),
          ),
        ),
        if (widget.kind == 'albums' && wide)
          SliverToBoxAdapter(
            child: ContentWidth(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: LayoutBuilder(
                  builder: (context, c) {
                    final cols = (c.maxWidth / 190).floor().clamp(3, 6);
                    final w = (c.maxWidth - 16 * (cols - 1)) / cols;
                    return Wrap(
                      spacing: 16,
                      runSpacing: 18,
                      children: [for (final (i, a) in _items.indexed) AlbumCard(item: a, width: w, index: i % 50)],
                    );
                  },
                ),
              ),
            ),
          )
        else
          SliverList.builder(
            itemCount: _items.length,
            itemBuilder: (_, i) {
              final it = _items[i];
              final row = switch (widget.kind) {
                'tracks' => TrackRow(item: it, maxPlays: maxPlays, index: i % 50),
                'artists' => ArtistRow(item: it, maxPlays: maxPlays, index: i % 50),
                _ => AlbumRow(item: it, maxPlays: maxPlays, index: i % 50),
              };
              return ContentWidth(
                child: Padding(padding: const EdgeInsets.symmetric(horizontal: 12), child: row),
              );
            },
          ),
        SliverToBoxAdapter(
          child: _error != null
              ? ErrorBox(
                  error: _error!,
                  onRetry: () {
                    setState(() => _error = null);
                    _more();
                  },
                )
              : (_loading
                    ? const Loading()
                    : (_page > 0 && _items.isEmpty
                          ? const Padding(
                              padding: EdgeInsets.all(32),
                              child: Text(
                                T.vacioTitulo,
                                textAlign: TextAlign.center,
                                style: TextStyle(color: C.muted),
                              ),
                            )
                          : const SizedBox(height: 32))),
        ),
      ],
    );
  }
}
