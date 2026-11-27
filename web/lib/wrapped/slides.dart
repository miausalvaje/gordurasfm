import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../format.dart';
import '../l10n/wrapped_es.dart';
import '../theme.dart';
import 'kit.dart';
import 'model.dart';

/// Lo que recibe cada diapositiva.
class SlideArgs {
  const SlideArgs({required this.d, required this.t, required this.still, required this.colors});
  final WrappedData d;

  /// Animación de entrada (0 → 1). En modo quieto vale siempre 1.
  final Animation<double> t;

  /// Sin animaciones ni botones (para exportar a imagen).
  final bool still;
  final List<Color> colors;
}

/// Margen común de las diapositivas (deja sitio arriba a las barras de progreso y los botones).
class SlidePad extends StatelessWidget {
  const SlidePad({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context) => Padding(padding: const EdgeInsets.fromLTRB(26, 70, 26, 34), child: child);
}

/// Marca pequeña al pie (sale también en las imágenes exportadas).
class Footer extends StatelessWidget {
  const Footer({super.key, required this.year});
  final int year;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisAlignment: MainAxisAlignment.center,
    children: [
      Container(
        width: 16,
        height: 16,
        decoration: const BoxDecoration(gradient: C.gradient, shape: BoxShape.circle),
        child: const Icon(Icons.graphic_eq_rounded, size: 11, color: Colors.white),
      ),
      const SizedBox(width: 6),
      Text(
        'OwnFM · Wrapped $year',
        style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w700, color: Colors.white60, letterSpacing: 0.4),
      ),
    ],
  );
}

// ---------------------------------------------------------------- 1. Introducción

class IntroSlide extends StatelessWidget {
  const IntroSlide(this.a, {super.key});
  final SlideArgs a;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final covers = <String>{
      for (final i in [...d.topAlbums, ...d.topTracks, ...d.topArtists])
        if (i.image != null) i.image!,
    }.take(6).toList();
    Widget coverRow(List<String> urls, double size, int offset) => Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        for (var i = 0; i < urls.length; i++)
          Reveal(
            t: a.t,
            begin: 0.1 + (i + offset) * 0.07,
            end: 0.55 + (i + offset) * 0.07,
            scale: 0.4,
            dy: 0,
            child: Transform.translate(
              offset: Offset(0, i.isOdd ? size * 0.18 : 0),
              child: Transform.rotate(
                angle: (i.isEven ? -1 : 1) * (0.06 + 0.03 * i),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 5),
                  child: WImage(url: urls[i], seed: urls[i], size: size, shadow: true),
                ),
              ),
            ),
          ),
      ],
    );
    return SlidePad(
      child: Column(
        children: [
          if (covers.isNotEmpty) coverRow(covers.take(3).toList(), 88, 0),
          const Spacer(flex: 2),
          Reveal(
            t: a.t,
            end: 0.35,
            child: Text('Tu', style: wStyle(34, weight: FontWeight.w700)),
          ),
          Reveal(
            t: a.t,
            begin: 0.1,
            end: 0.55,
            scale: 0.6,
            child: Grad('${d.year}', style: wStyle(118, weight: FontWeight.w900, height: 1.1)),
          ),
          Reveal(
            t: a.t,
            begin: 0.25,
            end: 0.65,
            child: Text('en música', style: wStyle(34, weight: FontWeight.w700)),
          ),
          const Spacer(flex: 2),
          Reveal(
            t: a.t,
            begin: 0.5,
            end: 0.9,
            child: Text(
              '${TW.hola(d.user)}\n${TW.intro2}',
              textAlign: TextAlign.center,
              style: wSub.copyWith(fontSize: 16),
            ),
          ),
          const Spacer(),
          if (covers.length > 3) ...[coverRow(covers.skip(3).toList(), 60, 3), const SizedBox(height: 18)],
          Footer(year: d.year),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------- 2. Escuchas

class PlaysSlide extends StatelessWidget {
  const PlaysSlide(this.a, {super.key});
  final SlideArgs a;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    // Media sobre los días con historial: desde la primera escucha del año (si el historial empieza a
    // mitad de año) hasta hoy o hasta el final del año.
    final now = DateTime.now();
    final first = d.firstPlay == null ? DateTime(d.year) : DateTime.fromMillisecondsSinceEpoch(d.firstPlay! * 1000);
    final start = first.isAfter(DateTime(d.year)) ? DateTime(first.year, first.month, first.day) : DateTime(d.year);
    final end = now.year == d.year ? now : DateTime(d.year + 1);
    final days = math.max(1, end.difference(start).inDays + (now.year == d.year ? 1 : 0));
    final pct = d.changePct;
    return SlidePad(
      child: Column(
        children: [
          const Spacer(flex: 3),
          Reveal(
            t: a.t,
            end: 0.3,
            child: Text(TW.lePisteAlPlay(d.year), style: wSub.copyWith(fontSize: 18)),
          ),
          const SizedBox(height: 10),
          Reveal(
            t: a.t,
            begin: 0.05,
            end: 0.4,
            scale: 0.7,
            child: FittedBox(
              child: CountUp(
                t: a.t,
                value: d.plays,
                style: wStyle(96, weight: FontWeight.w900, height: 1),
              ),
            ),
          ),
          Reveal(
            t: a.t,
            begin: 0.2,
            end: 0.5,
            child: Text(TW.veces, style: wStyle(30, weight: FontWeight.w700)),
          ),
          const Spacer(),
          if (pct != null && (d.previousPlays ?? 0) > 0)
            Reveal(
              t: a.t,
              begin: 0.6,
              end: 0.9,
              child: Pill(
                TW.frenteA('${fmtInt(pct.abs().round())} %', pct >= 0, d.year - 1),
                icon: pct >= 0 ? Icons.trending_up_rounded : Icons.trending_down_rounded,
                color: pct >= 0 ? const Color(0xFF7CF5B5) : const Color(0xFFFFB4A2),
              ),
            ),
          const SizedBox(height: 14),
          Reveal(
            t: a.t,
            begin: 0.7,
            end: 1,
            child: Text(TW.mediaDia(fmtDecimal(d.plays / days)), style: wSub),
          ),
          const Spacer(flex: 2),
          Footer(year: d.year),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------- 3. Tiempo

class TimeSlide extends StatelessWidget {
  const TimeSlide(this.a, {super.key});
  final SlideArgs a;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final mins = d.minutes;
    final totalDays = DateTime(d.year + 1).difference(DateTime(d.year)).inDays;
    final equiv = mins >= 1440 ? TW.diasSeguidos(fmtDecimal(mins / 1440)) : TW.horas(fmtDecimal(mins / 60));
    return SlidePad(
      child: Column(
        children: [
          const Spacer(flex: 3),
          Reveal(
            t: a.t,
            end: 0.3,
            child: Text(TW.tePasaste, style: wSub.copyWith(fontSize: 18)),
          ),
          const SizedBox(height: 10),
          Reveal(
            t: a.t,
            begin: 0.05,
            end: 0.4,
            scale: 0.7,
            child: FittedBox(
              child: CountUp(
                t: a.t,
                value: mins,
                style: wStyle(92, weight: FontWeight.w900, height: 1),
              ),
            ),
          ),
          Reveal(
            t: a.t,
            begin: 0.2,
            end: 0.5,
            child: Text(
              TW.minutosEscuchando,
              textAlign: TextAlign.center,
              style: wStyle(24, weight: FontWeight.w700, height: 1.15),
            ),
          ),
          if (d.msEstimated)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Text(TW.aprox, style: wSub.copyWith(fontSize: 12, color: Colors.white54)),
            ),
          const Spacer(),
          Reveal(
            t: a.t,
            begin: 0.55,
            end: 0.85,
            child: Grad(equiv, style: wStyle(19, weight: FontWeight.w800)),
          ),
          const SizedBox(height: 16),
          Reveal(
            t: a.t,
            begin: 0.65,
            end: 0.95,
            child: Text(TW.diasConMusica(d.daysWithMusic, totalDays), style: wSub),
          ),
          if (d.peakHour != null) ...[
            const SizedBox(height: 6),
            Reveal(
              t: a.t,
              begin: 0.72,
              end: 1,
              child: Text(TW.horaPunta(d.peakHour!), style: wSub),
            ),
          ],
          const Spacer(flex: 2),
          Footer(year: d.year),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------- 5, 7, 9. Ganadores

class WinnerSlide extends StatelessWidget {
  const WinnerSlide(this.a, {super.key, required this.kind});
  final SlideArgs a;
  final String kind;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final list = switch (kind) {
      'artist' => d.topArtists,
      'album' => d.topAlbums,
      _ => d.topTracks,
    };
    final w = list.first;
    final pct = d.plays == 0 ? 0.0 : w.plays * 100 / d.plays;
    final glow = a.colors.first;
    return SlidePad(
      child: Column(
        children: [
          Reveal(
            t: a.t,
            end: 0.3,
            child: Text(TW.ganador[kind]!, style: wSub.copyWith(fontSize: 18)),
          ),
          const Spacer(),
          Reveal(
            t: a.t,
            begin: 0.05,
            end: 0.55,
            scale: 0.5,
            dy: 0,
            child: Container(
              decoration: BoxDecoration(
                shape: w.round ? BoxShape.circle : BoxShape.rectangle,
                borderRadius: w.round ? null : BorderRadius.circular(18),
                boxShadow: [BoxShadow(color: glow.withValues(alpha: 0.55), blurRadius: 60, spreadRadius: 4)],
              ),
              child: Transform.rotate(
                angle: w.round ? 0 : -0.03,
                child: WImage(url: w.image, covers: w.covers, seed: w.title, size: 230, round: w.round, radius: 18),
              ),
            ),
          ),
          const Spacer(),
          Reveal(
            t: a.t,
            begin: 0.35,
            end: 0.7,
            child: FitText(w.title, style: wStyle(44, weight: FontWeight.w900)),
          ),
          if (w.subtitle != null) ...[
            const SizedBox(height: 4),
            Reveal(
              t: a.t,
              begin: 0.42,
              end: 0.75,
              child: Text(w.subtitle!, style: wSub.copyWith(fontSize: 17), textAlign: TextAlign.center, maxLines: 1),
            ),
          ],
          const SizedBox(height: 16),
          Reveal(
            t: a.t,
            begin: 0.5,
            end: 0.85,
            child: Grad(
              kind == 'track' ? TW.laPusiste(w.plays) : TW.escuchas(w.plays),
              style: wStyle(26, weight: FontWeight.w900),
            ),
          ),
          if (kind == 'artist') ...[
            const SizedBox(height: 6),
            Reveal(
              t: a.t,
              begin: 0.6,
              end: 0.95,
              child: Text(TW.pctDeTodo(fmtDecimal(pct)), style: wSub),
            ),
          ],
          const Spacer(),
          Footer(year: d.year),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------- 10-12. Top 5

class TopListSlide extends StatelessWidget {
  const TopListSlide(this.a, {super.key, required this.kind});
  final SlideArgs a;
  final String kind;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final list = switch (kind) {
      'artist' => d.topArtists,
      'album' => d.topAlbums,
      _ => d.topTracks,
    };
    return SlidePad(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Reveal(
            t: a.t,
            end: 0.3,
            child: Text('${d.year}', style: wSub.copyWith(fontSize: 16)),
          ),
          Reveal(
            t: a.t,
            begin: 0.05,
            end: 0.35,
            child: Text(TW.top5[kind]!, style: wStyle(40, weight: FontWeight.w900)),
          ),
          const Spacer(),
          for (var i = 0; i < list.length; i++) ...[
            Reveal(
              t: a.t,
              begin: 0.2 + i * 0.1,
              end: 0.55 + i * 0.1,
              dx: 40,
              dy: 0,
              child: _Row(item: list[i], first: i == 0),
            ),
            const SizedBox(height: 14),
          ],
          const Spacer(),
          Footer(year: d.year),
        ],
      ),
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({required this.item, required this.first});
  final WItem item;
  final bool first;

  @override
  Widget build(BuildContext context) {
    final size = first ? 70.0 : 56.0;
    return Row(
      children: [
        SizedBox(
          width: 34,
          child: first
              ? Grad('${item.rank}', style: wStyle(30, weight: FontWeight.w900))
              : Text(
                  '${item.rank}',
                  style: wStyle(26, weight: FontWeight.w900, color: Colors.white70),
                ),
        ),
        WImage(url: item.image, covers: item.covers, seed: item.title, size: size, round: item.round, shadow: true),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                item.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: first ? 19 : 16, fontWeight: FontWeight.w800, color: Colors.white),
              ),
              if (item.subtitle != null)
                Text(
                  item.subtitle!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 13, color: Colors.white70, fontWeight: FontWeight.w500),
                ),
              Text(
                TW.escuchas(item.plays),
                style: const TextStyle(fontSize: 12.5, color: Color(0xFFFFC27A), fontWeight: FontWeight.w700),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------- 13. Evolución

class EvolutionSlide extends StatelessWidget {
  const EvolutionSlide(this.a, {super.key});
  final SlideArgs a;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final maxPlays = d.monthly.fold<int>(1, (m, e) => math.max(m, e.plays));
    return SlidePad(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Reveal(
            t: a.t,
            end: 0.3,
            child: Text(TW.asiFue, style: wStyle(36, weight: FontWeight.w900)),
          ),
          const SizedBox(height: 18),
          SizedBox(
            height: 150,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                for (final m in d.monthly)
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 2.5),
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.end,
                        children: [
                          AnimatedBuilder(
                            animation: a.t,
                            builder: (_, _) {
                              final v = Curves.easeOutCubic.transform(
                                ((a.t.value - 0.1 - m.month * 0.025) / 0.4).clamp(0.0, 1.0),
                              );
                              final h = math.max(3.0, 124 * m.plays / maxPlays * v);
                              return Container(
                                height: h,
                                decoration: BoxDecoration(
                                  borderRadius: BorderRadius.circular(5),
                                  gradient: m.month == d.topMonth
                                      ? const LinearGradient(
                                          colors: [Color(0xFFFFD166), C.accent],
                                          begin: Alignment.topCenter,
                                          end: Alignment.bottomCenter,
                                        )
                                      : null,
                                  color: m.month == d.topMonth ? null : Colors.white.withValues(alpha: 0.32),
                                ),
                              );
                            },
                          ),
                          const SizedBox(height: 6),
                          Text(
                            mesesCortos[m.month - 1].substring(0, 1).toUpperCase(),
                            style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w700, color: Colors.white70),
                          ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
          ),
          const Spacer(),
          for (var i = 0; i < d.evolution.length; i++) ...[
            Reveal(
              t: a.t,
              begin: 0.35 + i * 0.1,
              end: 0.7 + i * 0.1,
              dx: 30,
              dy: 0,
              child: _QuarterRow(q: d.evolution[i]),
            ),
            const SizedBox(height: 12),
          ],
          const Spacer(),
          Footer(year: d.year),
        ],
      ),
    );
  }
}

class _QuarterRow extends StatelessWidget {
  const _QuarterRow({required this.q});
  final WQuarter q;

  @override
  Widget build(BuildContext context) {
    final top = q.artists.isEmpty ? '—' : q.artists.first;
    return Row(
      children: [
        WImage(url: q.image, covers: q.covers, seed: top, size: 46, round: true),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                TW.trimestres[q.quarter - 1],
                style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700, color: Color(0xFFFFC27A)),
              ),
              Text(
                top,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w800, color: Colors.white),
              ),
              if (q.artists.length > 1)
                Text(
                  q.artists.skip(1).join(' · '),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 12.5, color: Colors.white60),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------- 14. Mes

class MonthSlide extends StatelessWidget {
  const MonthSlide(this.a, {super.key});
  final SlideArgs a;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final m = d.topMonth ?? 1;
    return SlidePad(
      child: Column(
        children: [
          const Spacer(),
          Reveal(
            t: a.t,
            end: 0.3,
            child: Text(TW.tuMes, style: wSub.copyWith(fontSize: 18)),
          ),
          const SizedBox(height: 6),
          Reveal(
            t: a.t,
            begin: 0.08,
            end: 0.45,
            scale: 0.6,
            child: FittedBox(
              child: Grad(meses[m - 1], style: wStyle(84, weight: FontWeight.w900)),
            ),
          ),
          Reveal(
            t: a.t,
            begin: 0.25,
            end: 0.55,
            child: Text(TW.escuchas(d.topMonthPlays), style: wStyle(22, weight: FontWeight.w800)),
          ),
          const Spacer(),
          Reveal(
            t: a.t,
            begin: 0.45,
            end: 0.75,
            child: Text(TW.loQueMas.toUpperCase(), style: wSub.copyWith(fontSize: 12, letterSpacing: 1.2)),
          ),
          const SizedBox(height: 12),
          if (d.topMonthArtist != null)
            Reveal(
              t: a.t,
              begin: 0.5,
              end: 0.85,
              dx: -30,
              dy: 0,
              child: _MiniCard(item: d.topMonthArtist!),
            ),
          const SizedBox(height: 10),
          if (d.topMonthTrack != null)
            Reveal(
              t: a.t,
              begin: 0.6,
              end: 0.95,
              dx: 30,
              dy: 0,
              child: _MiniCard(item: d.topMonthTrack!),
            ),
          const Spacer(),
          Footer(year: d.year),
        ],
      ),
    );
  }
}

class _MiniCard extends StatelessWidget {
  const _MiniCard({required this.item});
  final WItem item;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.all(10),
    decoration: BoxDecoration(
      color: Colors.white.withValues(alpha: 0.1),
      borderRadius: BorderRadius.circular(16),
      border: Border.all(color: Colors.white.withValues(alpha: 0.12)),
    ),
    child: Row(
      children: [
        WImage(url: item.image, covers: item.covers, seed: item.title, size: 54, round: item.round),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                item.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w800, color: Colors.white),
              ),
              Text(
                [?item.subtitle, TW.escuchas(item.plays)].join(' · '),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 12.5, color: Colors.white70),
              ),
            ],
          ),
        ),
      ],
    ),
  );
}

// ---------------------------------------------------------------- 15. Resumen

class SummarySlide extends StatelessWidget {
  const SummarySlide(
    this.a, {
    super.key,
    required this.score,
    required this.quizzes,
    this.onShare,
    this.onExportAll,
    this.onReplay,
  });

  final SlideArgs a;
  final int score;
  final int quizzes;
  final VoidCallback? onShare;
  final VoidCallback? onExportAll;
  final VoidCallback? onReplay;

  @override
  Widget build(BuildContext context) {
    final d = a.d;
    final hero = d.topArtists.isEmpty ? null : d.topArtists.first;
    Widget col(String title, List<WItem> items) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            title,
            style: const TextStyle(fontSize: 11.5, fontWeight: FontWeight.w700, color: Colors.white60),
          ),
          const SizedBox(height: 4),
          for (final i in items.take(5))
            Padding(
              padding: const EdgeInsets.only(top: 3),
              child: Text(
                '${i.rank}  ${i.title}',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontSize: 13.5, fontWeight: FontWeight.w700, color: Colors.white),
              ),
            ),
        ],
      ),
    );
    Widget stat(String title, String v) => Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            title,
            style: const TextStyle(fontSize: 11.5, fontWeight: FontWeight.w700, color: Colors.white60),
          ),
          FittedBox(
            child: Text(v, style: wStyle(28, weight: FontWeight.w900)),
          ),
        ],
      ),
    );
    return SlidePad(
      child: Column(
        children: [
          const Spacer(),
          Reveal(
            t: a.t,
            end: 0.5,
            scale: 0.9,
            child: Container(
              padding: const EdgeInsets.all(18),
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(24),
                gradient: LinearGradient(
                  colors: [Color.lerp(a.colors[0], Colors.black, 0.35)!, Color.lerp(a.colors[1], Colors.black, 0.55)!],
                  begin: Alignment.topLeft,
                  end: Alignment.bottomRight,
                ),
                border: Border.all(color: Colors.white.withValues(alpha: 0.15)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      if (hero != null)
                        WImage(url: hero.image, covers: hero.covers, seed: hero.title, size: 74, round: true),
                      const SizedBox(width: 14),
                      Expanded(
                        child: Text(TW.resumenTitulo(d.year), style: wStyle(24, weight: FontWeight.w900, height: 1.1)),
                      ),
                    ],
                  ),
                  const SizedBox(height: 16),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      col(TW.topArtistas, d.topArtists),
                      const SizedBox(width: 10),
                      col(TW.topCanciones, d.topTracks),
                    ],
                  ),
                  const SizedBox(height: 16),
                  Row(children: [stat(TW.minutos, fmtInt(d.minutes)), stat(TW.escuchasTit, fmtInt(d.plays))]),
                  if (quizzes > 0) ...[
                    const SizedBox(height: 12),
                    Center(
                      child: Pill(
                        TW.acertaste(score, quizzes),
                        icon: Icons.emoji_events_rounded,
                        color: const Color(0xFFFFD166),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
          const Spacer(),
          if (!a.still)
            Reveal(
              t: a.t,
              begin: 0.5,
              end: 0.9,
              child: Column(
                children: [
                  SizedBox(
                    width: double.infinity,
                    child: FilledButton.icon(
                      style: FilledButton.styleFrom(
                        backgroundColor: Colors.white,
                        foregroundColor: Colors.black,
                        padding: const EdgeInsets.symmetric(vertical: 14),
                      ),
                      onPressed: onShare,
                      icon: const Icon(Icons.ios_share_rounded),
                      label: const Text(TW.compartir, style: TextStyle(fontWeight: FontWeight.w800)),
                    ),
                  ),
                  const SizedBox(height: 8),
                  Row(
                    children: [
                      Expanded(
                        child: OutlinedButton.icon(
                          style: OutlinedButton.styleFrom(
                            foregroundColor: Colors.white,
                            side: const BorderSide(color: Colors.white54),
                            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 12),
                            textStyle: const TextStyle(fontSize: 13.5, fontWeight: FontWeight.w700),
                          ),
                          onPressed: onExportAll,
                          icon: const Icon(Icons.collections_rounded, size: 18),
                          label: const Text(TW.exportarTodo),
                        ),
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: OutlinedButton.icon(
                          style: OutlinedButton.styleFrom(
                            foregroundColor: Colors.white,
                            side: const BorderSide(color: Colors.white54),
                            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 12),
                            textStyle: const TextStyle(fontSize: 13.5, fontWeight: FontWeight.w700),
                          ),
                          onPressed: onReplay,
                          icon: const Icon(Icons.replay_rounded, size: 18),
                          label: const Text(TW.otraVez),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            )
          else
            Footer(year: d.year),
        ],
      ),
    );
  }
}
