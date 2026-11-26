import 'dart:async';
import 'dart:math' as math;

import 'package:fl_chart/fl_chart.dart';
import 'package:flutter_animate/flutter_animate.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../l10n/es.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';
import '../widgets/top_items.dart';

/// Portada: estadísticas del periodo elegido, gráfica y los tres tops.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.period});
  final String period;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  StatsData? _data;
  Object? _error;
  bool _loading = false;
  NowPlaying? _now;
  Timer? _timer;
  int _req = 0;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_data == null && !_loading) {
      _load();
      _pollNow();
      _timer ??= Timer.periodic(const Duration(seconds: 30), (_) => _pollNow());
    }
  }

  @override
  void didUpdateWidget(HomeScreen old) {
    super.didUpdateWidget(old);
    if (old.period != widget.period) _load();
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    final req = ++_req;
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final d = await context.api.stats(widget.period);
      if (!mounted || req != _req) return;
      setState(() => _data = d);
    } catch (e) {
      if (!mounted || req != _req) return;
      setState(() => _error = e);
    } finally {
      if (mounted && req == _req) setState(() => _loading = false);
    }
  }

  Future<void> _pollNow() async {
    try {
      final n = await context.api.nowPlaying();
      if (mounted) setState(() => _now = n);
    } catch (_) {}
  }

  void _setPeriod(String p) => context.go(p == 'this_week' ? '/' : '/?periodo=$p');

  @override
  Widget build(BuildContext context) {
    final user = context.session.user;
    final d = _data;
    return RefreshIndicator(
      onRefresh: _load,
      child: ListView(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
        children: [
          ContentWidth(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (user != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 14),
                    child: Text(
                      T.hola(user.displayName),
                      style: const TextStyle(fontFamily: 'InterDisplay', fontSize: 28, fontWeight: FontWeight.w900),
                    ),
                  ),
                if (_now != null) ...[_NowPlayingCard(now: _now!), const SizedBox(height: 14)],
                PeriodChips(value: widget.period, onChanged: _setPeriod),
                const SizedBox(height: 16),
                AnimatedOpacity(
                  opacity: _loading && d != null ? 0.55 : 1,
                  duration: const Duration(milliseconds: 150),
                  child: _body(d),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _body(StatsData? d) {
    if (d == null) {
      if (_error != null) return ErrorBox(error: _error!, onRetry: _load);
      return const Loading();
    }
    if (_error != null) {
      // Si falla un cambio de periodo, se sigue enseñando lo anterior con aviso.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && _error != null) toast(context, _error.toString(), error: true);
      });
    }
    if (d.totals.plays == 0) return const _Empty();
    final wide = MediaQuery.sizeOf(context).width >= 1000;
    final q = widget.period == 'this_week' ? '' : '?periodo=${widget.period}';
    final albums = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionHeader(title: T.topAlbums, action: T.verTodos, onAction: () => context.go('/top/albums$q')),
        _AlbumStrip(items: d.topAlbums),
      ],
    );
    final tracks = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionHeader(title: T.topTracks, action: T.verTodos, onAction: () => context.go('/top/tracks$q')),
        Panel(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Column(
            children: [
              for (final (i, t) in d.topTracks.indexed) TrackRow(item: t, maxPlays: d.topTracks.first.plays, index: i),
            ],
          ),
        ),
      ],
    );
    final artists = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SectionHeader(title: T.topArtists, action: T.verTodos, onAction: () => context.go('/top/artists$q')),
        if (wide)
          Panel(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: Column(
              children: [
                for (final (i, a) in d.topArtists.indexed)
                  ArtistRow(item: a, maxPlays: d.topArtists.first.plays, index: i),
              ],
            ),
          )
        else
          SizedBox(
            height: 160,
            child: ListView.separated(
              scrollDirection: Axis.horizontal,
              clipBehavior: Clip.none,
              itemCount: d.topArtists.length,
              separatorBuilder: (_, _) => const SizedBox(width: 14),
              itemBuilder: (_, i) => ArtistBubble(item: d.topArtists[i], index: i),
            ),
          ),
      ],
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Totals(d: d),
        const SizedBox(height: 14),
        Panel(
          title: T.porUnidad(d.unit),
          child: SizedBox(
            height: 220,
            child: PlaysChart(points: d.chart, unit: d.unit),
          ),
        ),
        albums,
        if (wide)
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(flex: 3, child: tracks),
              const SizedBox(width: 20),
              Expanded(flex: 2, child: artists),
            ],
          )
        else ...[
          tracks,
          artists,
        ],
        const SizedBox(height: 28),
        Panel(
          title: T.cuandoEscuchas,
          child: SizedBox(height: 130, child: HourChart(byHour: d.totals.byHour)),
        ),
      ],
    );
  }
}

class _NowPlayingCard extends StatelessWidget {
  const _NowPlayingCard({required this.now});
  final NowPlaying now;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(14),
    decoration: BoxDecoration(
      borderRadius: BorderRadius.circular(16),
      gradient: LinearGradient(colors: [C.accent.withValues(alpha: 0.22), C.accent2.withValues(alpha: 0.12)]),
      border: Border.all(color: C.accent.withValues(alpha: 0.35)),
    ),
    child: Row(
      children: [
        const Icon(
          Icons.graphic_eq_rounded,
          color: C.accent,
        ).animate(onPlay: (c) => c.repeat(reverse: true)).scaleY(begin: 0.7, end: 1.1, duration: 700.ms),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                T.escuchandoAhora,
                style: TextStyle(color: C.accent, fontSize: 12, fontWeight: FontWeight.w700),
              ),
              Text(
                now.track,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontWeight: FontWeight.w700),
              ),
              Text(
                [now.artist, now.album].whereType<String>().join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: C.muted, fontSize: 13),
              ),
            ],
          ),
        ),
      ],
    ),
  );
}

class _Totals extends StatelessWidget {
  const _Totals({required this.d});
  final StatsData d;

  @override
  Widget build(BuildContext context) {
    final t = d.totals;
    final change = t.changePct;
    final tiles = <(String, String, IconData, String?)>[
      (
        T.tiempoEscuchado,
        '${t.withoutDuration > 0 ? '≈ ' : ''}${fmtDuration(t.listeningMs)}',
        Icons.schedule_rounded,
        t.withoutDuration > 0 ? T.sinDuracion : null,
      ),
      (T.artistas, fmtInt(t.artists), Icons.mic_external_on_rounded, null),
      (T.albumes, fmtInt(t.albums), Icons.album_rounded, null),
      (T.canciones, fmtInt(t.tracks), Icons.music_note_rounded, null),
      (T.mediaDiaria, fmtDecimal(t.avgPerDay), Icons.today_rounded, null),
      if (t.topDay != null)
        (
          T.diaTop,
          '${fmtShortDate(DateTime.parse(t.topDay!.$1))} · ${fmtInt(t.topDay!.$2)}',
          Icons.local_fire_department_rounded,
          null,
        ),
    ];
    return LayoutBuilder(
      builder: (context, c) {
        final cols = c.maxWidth >= 900 ? 3 : 2;
        final gap = 10.0;
        final w = (c.maxWidth - gap * (cols - 1)) / cols;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              padding: const EdgeInsets.all(20),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(20),
                gradient: const LinearGradient(
                  colors: [Color(0xFF2A1420), Color(0xFF1B1520)],
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                ),
                border: Border.all(color: C.accent.withValues(alpha: 0.25)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    T.periodos[d.period]!.toUpperCase(),
                    style: const TextStyle(
                      color: C.muted,
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 1.2,
                    ),
                  ),
                  const SizedBox(height: 6),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.baseline,
                    textBaseline: TextBaseline.alphabetic,
                    children: [
                      GradientText(
                        fmtInt(t.plays),
                        style: const TextStyle(
                          fontFamily: 'InterDisplay',
                          fontSize: 52,
                          fontWeight: FontWeight.w900,
                          height: 1,
                        ),
                      ),
                      const SizedBox(width: 10),
                      const Text(T.escuchas, style: TextStyle(fontSize: 18, fontWeight: FontWeight.w600)),
                    ],
                  ),
                  if (change != null) ...[
                    const SizedBox(height: 10),
                    Row(
                      children: [
                        Container(
                          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                          decoration: BoxDecoration(
                            color: (change >= 0 ? C.ok : C.bad).withValues(alpha: 0.15),
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Icon(
                                change >= 0 ? Icons.trending_up_rounded : Icons.trending_down_rounded,
                                size: 16,
                                color: change >= 0 ? C.ok : C.bad,
                              ),
                              const SizedBox(width: 4),
                              Text(
                                fmtChange(change),
                                style: TextStyle(
                                  color: change >= 0 ? C.ok : C.bad,
                                  fontWeight: FontWeight.w700,
                                  fontSize: 13,
                                ),
                              ),
                            ],
                          ),
                        ),
                        const SizedBox(width: 8),
                        const Flexible(
                          child: Text(T.frenteAnterior, style: TextStyle(color: C.muted, fontSize: 13)),
                        ),
                      ],
                    ),
                  ],
                ],
              ),
            ).animate().fadeIn(duration: 300.ms),
            const SizedBox(height: 10),
            Wrap(
              spacing: gap,
              runSpacing: gap,
              children: [
                for (final (i, tile) in tiles.indexed)
                  SizedBox(
                    width: w,
                    child: _StatTile(label: tile.$1, value: tile.$2, icon: tile.$3, hint: tile.$4),
                  ).animate().fadeIn(duration: 300.ms, delay: (40 * i).ms),
              ],
            ),
          ],
        );
      },
    );
  }
}

class _StatTile extends StatelessWidget {
  const _StatTile({required this.label, required this.value, required this.icon, this.hint});
  final String label;
  final String value;
  final IconData icon;
  final String? hint;

  @override
  Widget build(BuildContext context) {
    final tile = Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(color: C.surface, borderRadius: BorderRadius.circular(16)),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, size: 16, color: C.accent2),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: C.muted, fontSize: 12.5),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(
              fontSize: 19,
              fontWeight: FontWeight.w700,
              fontFeatures: [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
    return hint == null ? tile : Tooltip(message: hint!, child: tile);
  }
}

/// Gráfica de barras de escuchas por día, mes o año.
class PlaysChart extends StatelessWidget {
  const PlaysChart({super.key, required this.points, required this.unit});
  final List<ChartPoint> points;
  final String unit;

  @override
  Widget build(BuildContext context) {
    final maxY = points.fold<int>(0, (m, p) => math.max(m, p.plays));
    final top = maxY == 0 ? 1.0 : (maxY * 1.15);
    final n = points.length;
    return LayoutBuilder(
      builder: (context, c) {
        final barW = ((c.maxWidth - 30) / n * 0.62).clamp(3.0, 28.0);
        return BarChart(
          BarChartData(
            maxY: top,
            alignment: BarChartAlignment.spaceAround,
            gridData: FlGridData(
              drawVerticalLine: false,
              horizontalInterval: _niceStep(top),
              getDrawingHorizontalLine: (_) => const FlLine(color: C.line, strokeWidth: 1, dashArray: [4, 4]),
            ),
            borderData: FlBorderData(show: false),
            titlesData: FlTitlesData(
              topTitles: const AxisTitles(),
              rightTitles: const AxisTitles(),
              leftTitles: AxisTitles(
                sideTitles: SideTitles(
                  showTitles: true,
                  reservedSize: 34,
                  interval: _niceStep(top),
                  getTitlesWidget: (v, meta) => v == 0 || v >= top
                      ? const SizedBox.shrink()
                      : SideTitleWidget(
                          meta: meta,
                          child: Text(_compact(v), style: const TextStyle(color: C.muted, fontSize: 10.5)),
                        ),
                ),
              ),
              bottomTitles: AxisTitles(
                sideTitles: SideTitles(
                  showTitles: true,
                  reservedSize: 24,
                  getTitlesWidget: (v, meta) {
                    final i = v.toInt();
                    if (i < 0 || i >= n) return const SizedBox.shrink();
                    return SideTitleWidget(
                      meta: meta,
                      child: Text(
                        chartTick(points[i].label, unit, n, i),
                        style: const TextStyle(color: C.muted, fontSize: 11),
                      ),
                    );
                  },
                ),
              ),
            ),
            barTouchData: BarTouchData(
              touchTooltipData: BarTouchTooltipData(
                getTooltipColor: (_) => C.surface2,
                tooltipBorderRadius: BorderRadius.circular(10),
                getTooltipItem: (group, _, rod, _) => BarTooltipItem(
                  '${chartLabel(points[group.x].label, unit)}\n',
                  const TextStyle(color: C.muted, fontSize: 12),
                  children: [
                    TextSpan(
                      text: '${fmtInt(rod.toY)} ${T.escuchas}',
                      style: const TextStyle(color: C.text, fontWeight: FontWeight.w700, fontSize: 13),
                    ),
                  ],
                ),
              ),
            ),
            barGroups: [
              for (var i = 0; i < n; i++)
                BarChartGroupData(
                  x: i,
                  barRods: [
                    BarChartRodData(
                      toY: points[i].plays.toDouble(),
                      width: barW,
                      borderRadius: BorderRadius.vertical(top: Radius.circular(math.min(6, barW / 2))),
                      gradient: const LinearGradient(
                        colors: [C.accent, C.accent2],
                        begin: Alignment.bottomCenter,
                        end: Alignment.topCenter,
                      ),
                    ),
                  ],
                ),
            ],
          ),
          duration: const Duration(milliseconds: 350),
        );
      },
    );
  }
}

double _niceStep(double top) {
  final raw = top / 4;
  if (raw <= 1) return 1;
  final mag = math.pow(10, (math.log(raw) / math.ln10).floor()).toDouble();
  for (final m in [1, 2, 2.5, 5, 10]) {
    if (raw <= m * mag) return m * mag;
  }
  return 10 * mag;
}

String _compact(double v) =>
    v >= 10000 ? '${(v / 1000).round()}k' : (v >= 1000 ? '${fmtDecimal(v / 1000)}k' : fmtInt(v));

/// Escuchas por hora del día (0-23).
class HourChart extends StatelessWidget {
  const HourChart({super.key, required this.byHour});
  final List<int> byHour;

  @override
  Widget build(BuildContext context) {
    final maxV = byHour.fold<int>(0, math.max);
    return BarChart(
      BarChartData(
        maxY: maxV == 0 ? 1 : maxV * 1.1,
        gridData: const FlGridData(show: false),
        borderData: FlBorderData(show: false),
        titlesData: FlTitlesData(
          topTitles: const AxisTitles(),
          rightTitles: const AxisTitles(),
          leftTitles: const AxisTitles(),
          bottomTitles: AxisTitles(
            sideTitles: SideTitles(
              showTitles: true,
              reservedSize: 20,
              getTitlesWidget: (v, meta) => v.toInt() % 6 == 0
                  ? SideTitleWidget(
                      meta: meta,
                      child: Text('${v.toInt()} h', style: const TextStyle(color: C.muted, fontSize: 10.5)),
                    )
                  : const SizedBox.shrink(),
            ),
          ),
        ),
        barTouchData: BarTouchData(
          touchTooltipData: BarTouchTooltipData(
            getTooltipColor: (_) => C.surface2,
            getTooltipItem: (g, _, rod, _) => BarTooltipItem(
              '${g.x}:00 · ${fmtInt(rod.toY)}',
              const TextStyle(color: C.text, fontSize: 12, fontWeight: FontWeight.w600),
            ),
          ),
        ),
        barGroups: [
          for (var h = 0; h < byHour.length; h++)
            BarChartGroupData(
              x: h,
              barRods: [
                BarChartRodData(
                  toY: byHour[h].toDouble(),
                  width: 7,
                  color: byHour[h] == maxV && maxV > 0 ? C.accent : C.accent.withValues(alpha: 0.35),
                  borderRadius: BorderRadius.circular(3),
                ),
              ],
            ),
        ],
      ),
    );
  }
}

/// Fila horizontal de álbumes (móvil) o cuadrícula (escritorio).
class _AlbumStrip extends StatelessWidget {
  const _AlbumStrip({required this.items});
  final List<TopItem> items;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, c) {
      if (c.maxWidth < 700) {
        return SizedBox(
          height: 222,
          child: ListView.separated(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            itemCount: items.length,
            separatorBuilder: (_, _) => const SizedBox(width: 14),
            itemBuilder: (_, i) => AlbumCard(item: items[i], index: i),
          ),
        );
      }
      final cols = (c.maxWidth / 190).floor().clamp(4, 6);
      final w = (c.maxWidth - 16 * (cols - 1)) / cols;
      return Wrap(
        spacing: 16,
        runSpacing: 18,
        children: [for (final (i, a) in items.indexed) AlbumCard(item: a, width: w, index: i)],
      );
    },
  );
}

class _Empty extends StatelessWidget {
  const _Empty();

  @override
  Widget build(BuildContext context) => Panel(
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 24),
      child: Column(
        children: [
          const Icon(Icons.headphones_rounded, size: 48, color: C.accent),
          const SizedBox(height: 14),
          const Text(T.vacioTitulo, style: TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
          const SizedBox(height: 8),
          const Text(
            T.vacioTexto,
            textAlign: TextAlign.center,
            style: TextStyle(color: C.muted),
          ),
          const SizedBox(height: 16),
          FilledButton(onPressed: () => context.go('/ajustes/conexiones'), child: const Text(T.irConexiones)),
        ],
      ),
    ),
  );
}
