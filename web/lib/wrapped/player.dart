import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../l10n/wrapped_es.dart';
import '../platform/music.dart';
import '../session.dart';
import '../widgets/common.dart';
import 'background.dart';
import 'kit.dart';
import 'model.dart';
import 'palette.dart';
import 'quiz.dart';
import 'slides.dart';
import 'zip.dart';

/// Segundos que dura cada diapositiva (y cada prueba, desde que se revela la respuesta).
const kSlideSeconds = 15;
const _slideDuration = Duration(seconds: kSlideSeconds);

/// `preserve`: con «reducir movimiento» activado, Flutter acorta las animaciones normales al 5 %
/// (15 s → 0,75 s). El de las diapositivas es un temporizador, no una animación (issue #2).
const _timerBehavior = AnimationBehavior.preserve;

class _Spec {
  const _Spec(this.kind, [this.q]);
  final String kind;

  /// Pregunta (artist, album, track) en las pruebas y en los ganadores.
  final String? q;

  bool get isQuiz => kind == 'quiz';
}

/// Reproductor del Wrapped: diapositivas tipo Stories a pantalla completa, con música de fondo.
class WrappedPlayerScreen extends StatefulWidget {
  const WrappedPlayerScreen({super.key, required this.year});
  final int year;

  @override
  State<WrappedPlayerScreen> createState() => _WrappedPlayerScreenState();
}

class _WrappedPlayerScreenState extends State<WrappedPlayerScreen> with TickerProviderStateMixin {
  WrappedData? _d;
  Object? _error;
  WPalette _pal = WPalette.fallback;
  bool _started = false;
  List<_Spec> _slides = const [];
  int _i = 0;
  late final AnimationController _intro = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1800),
  );

  /// Temporizador de cada diapositiva.
  late final AnimationController _progress;
  BackgroundMusic? _music;
  bool _muted = false;
  bool _held = false;

  /// Respuestas de esta visualización: pregunta → posición elegida.
  final Map<String, int> _answers = {};
  final _boundary = GlobalKey();
  final _focus = FocusNode();
  bool _still = false;
  String? _busyMsg;

  Api get api => context.api;

  @override
  void initState() {
    super.initState();
    _progress = AnimationController(vsync: this, duration: _slideDuration, animationBehavior: _timerBehavior);
    _progress.addStatusListener((s) {
      if (s == AnimationStatus.completed && _i < _slides.length - 1 && !_still) _go(_i + 1);
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_d == null && _error == null) _load();
  }

  @override
  void dispose() {
    _music?.dispose();
    _intro.dispose();
    _progress.dispose();
    _focus.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final d = WrappedData.fromJson(await api.get('/api/v1/wrapped/${widget.year}') as Map<String, dynamic>);
      final slides = <_Spec>[const _Spec('intro'), const _Spec('plays'), const _Spec('time')];
      for (final (q, list) in [('artist', d.topArtists), ('album', d.topAlbums), ('track', d.topTracks)]) {
        if (list.length >= 2) slides.add(_Spec('quiz', q));
        if (list.isNotEmpty) slides.add(_Spec('winner', q));
      }
      for (final (q, list) in [('artist', d.topArtists), ('track', d.topTracks), ('album', d.topAlbums)]) {
        if (list.isNotEmpty) slides.add(_Spec('top', q));
      }
      slides.add(const _Spec('evolution'));
      if (d.topMonth != null) slides.add(const _Spec('month'));
      slides.add(const _Spec('summary'));
      final pal = await paletteFrom([
        ?d.topAlbums.firstOrNull?.image,
        ?d.topArtists.firstOrNull?.image,
        ...?d.topArtists.firstOrNull?.covers,
        ?d.topTracks.firstOrNull?.image,
      ]);
      if (!mounted) return;
      setState(() {
        _d = d;
        _slides = slides;
        _pal = pal;
      });
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  // ---------- Navegación ----------

  _Spec get _spec => _slides[_i];

  bool get _blockedByQuiz => _spec.isQuiz && !_answers.containsKey(_spec.q) && !_still;

  void _start() {
    final d = _d!;
    // Dentro del toque: así el navegador deja sonar el audio.
    _music = BackgroundMusic([for (final s in d.soundtrack) '/api/v1/media/stream/${Uri.encodeComponent(s.id)}'])
      ..start();
    for (final u in d.allImages) {
      precacheImage(NetworkImage(u), context).catchError((_) {});
    }
    setState(() => _started = true);
    _focus.requestFocus();
    _go(0);
  }

  void _go(int i) {
    setState(() => _i = i.clamp(0, _slides.length - 1));
    _intro.forward(from: 0);
    if (_blockedByQuiz) {
      _progress.value = 0;
    } else if (_held) {
      _progress.value = 0;
    } else {
      _progress.forward(from: 0);
    }
  }

  void _next() {
    if (_blockedByQuiz) return;
    if (_i < _slides.length - 1) _go(_i + 1);
  }

  void _prev() => _go(_i > 0 ? _i - 1 : 0);

  void _hold(bool on) {
    _held = on;
    if (on) {
      _progress.stop();
    } else if (!_blockedByQuiz && !_progress.isCompleted) {
      _progress.forward();
    }
  }

  void _answer(String q, int rank) {
    setState(() => _answers[q] = rank);
    api.send('POST', '/api/v1/wrapped/answers/${_d!.id}', {'question': q, 'chosen_rank': rank}).catchError((_) => null);
  }

  void _quizRevealed() {
    if (!mounted || !_spec.isQuiz) return;
    if (!_held) _progress.forward(from: 0);
  }

  void _toggleMute() {
    setState(() => _muted = !_muted);
    _music?.setMuted(_muted);
  }

  void _replay() {
    _answers.clear();
    _go(0);
  }

  // ---------- Exportar ----------

  Future<void> _frames() async {
    await WidgetsBinding.instance.endOfFrame;
    await WidgetsBinding.instance.endOfFrame;
    await Future<void>.delayed(const Duration(milliseconds: 120));
  }

  Future<Uint8List> _capture() async {
    final b = _boundary.currentContext!.findRenderObject()! as RenderRepaintBoundary;
    final img = await b.toImage(pixelRatio: 1080 / kCanvasW);
    final data = await img.toByteData(format: ui.ImageByteFormat.png);
    img.dispose();
    return data!.buffer.asUint8List();
  }

  /// Pone las diapositivas quietas (sin animación ni botones, a 9:16), ejecuta `f` y lo deja como estaba.
  Future<T> _withStill<T>(Future<T> Function() f) async {
    _progress.stop();
    setState(() => _still = true);
    try {
      await _frames();
      return await f();
    } finally {
      if (mounted) {
        setState(() {
          _still = false;
          _busyMsg = null;
        });
        _intro.value = 1;
        if (!_blockedByQuiz && _i < _slides.length - 1) _progress.forward();
      }
    }
  }

  Future<void> _shareCurrent() async {
    final y = _d!.year;
    final png = await _withStill(_capture);
    await shareOrDownload(png, 'ownfm-wrapped-$y-${_i + 1}.png', 'image/png');
  }

  Future<void> _exportAll() async {
    final y = _d!.year;
    final back = _i;
    final files = await _withStill(() async {
      final out = <(String, Uint8List)>[];
      for (var j = 0; j < _slides.length; j++) {
        setState(() {
          _i = j;
          _busyMsg = TW.exportando(j + 1, _slides.length);
        });
        await _frames();
        out.add(('ownfm-wrapped-$y-${(j + 1).toString().padLeft(2, '0')}.png', await _capture()));
      }
      setState(() => _i = back);
      return out;
    });
    await shareOrDownload(zipStore(files), 'ownfm-wrapped-$y.zip', 'application/zip', share: false);
    if (mounted) toast(context, TW.exportado);
  }

  // ---------- Interfaz ----------

  Widget _slide(int i) {
    final d = _d!;
    final s = _slides[i];
    final a = SlideArgs(
      d: d,
      t: _still ? const AlwaysStoppedAnimation(1.0) : _intro,
      still: _still,
      colors: _pal.forSlide(i),
    );
    final content = switch (s.kind) {
      'intro' => IntroSlide(a),
      'plays' => PlaysSlide(a),
      'time' => TimeSlide(a),
      'quiz' => QuizSlide(
        a,
        kind: s.q!,
        items: switch (s.q) {
          'artist' => d.topArtists,
          'album' => d.topAlbums,
          _ => d.topTracks,
        },
        answered: _answers[s.q] ?? (_still ? d.answers[s.q]?.chosenRank : null),
        onAnswer: (r) => _answer(s.q!, r),
        onRevealed: _quizRevealed,
      ),
      'winner' => WinnerSlide(a, kind: s.q!),
      'top' => TopListSlide(a, kind: s.q!),
      'evolution' => EvolutionSlide(a),
      'month' => MonthSlide(a),
      _ => SummarySlide(
        a,
        score: _answers.values.where((r) => r == 1).length,
        quizzes: _slides.where((x) => x.isQuiz).length,
        onShare: _shareCurrent,
        onExportAll: _exportAll,
        onReplay: _replay,
      ),
    };
    return Stack(
      fit: StackFit.expand,
      children: [
        BlobBackground(colors: a.colors, seed: i * 7 + 3, still: _still),
        content,
      ],
    );
  }

  Widget _progressBars() => Row(
    children: [
      for (var j = 0; j < _slides.length; j++)
        Expanded(
          child: Container(
            height: 3,
            margin: const EdgeInsets.symmetric(horizontal: 1.5),
            decoration: BoxDecoration(color: Colors.white24, borderRadius: BorderRadius.circular(2)),
            alignment: Alignment.centerLeft,
            child: j < _i
                ? Container(color: Colors.white)
                : j > _i
                ? null
                : AnimatedBuilder(
                    animation: _progress,
                    builder: (_, _) => FractionallySizedBox(
                      widthFactor: _progress.value,
                      child: Container(
                        decoration: BoxDecoration(color: Colors.white, borderRadius: BorderRadius.circular(2)),
                      ),
                    ),
                  ),
          ),
        ),
    ],
  );

  Widget _iconButton(IconData icon, String tip, VoidCallback onTap) => IconButton(
    tooltip: tip,
    onPressed: onTap,
    icon: Icon(icon, color: Colors.white, size: 22),
    style: IconButton.styleFrom(backgroundColor: Colors.black.withValues(alpha: 0.2)),
  );

  Widget _canvas(double h) {
    final d = _d!;
    final stack = <Widget>[];
    if (!_started) {
      stack.add(
        Positioned.fill(
          child: _StartView(d: d, colors: _pal.forSlide(0), onStart: _start),
        ),
      );
    } else {
      final slide = _still
          ? KeyedSubtree(key: ValueKey('still$_i'), child: _slide(_i))
          : AnimatedSwitcher(
              duration: const Duration(milliseconds: 450),
              transitionBuilder: (child, anim) => FadeTransition(
                opacity: anim,
                child: ScaleTransition(scale: Tween(begin: 1.04, end: 1.0).animate(anim), child: child),
              ),
              child: KeyedSubtree(key: ValueKey(_i), child: _slide(_i)),
            );
      stack.addAll([
        Positioned.fill(
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapUp: (e) => e.localPosition.dx < kCanvasW * 0.3 ? _prev() : _next(),
            onLongPressStart: (_) => _hold(true),
            onLongPressEnd: (_) => _hold(false),
            child: RepaintBoundary(key: _boundary, child: slide),
          ),
        ),
        Positioned(top: 10, left: 10, right: 10, child: _progressBars()),
        Positioned(
          top: 20,
          right: 8,
          child: Row(
            children: [
              if (_music?.available ?? false)
                _iconButton(
                  _muted ? Icons.volume_off_rounded : Icons.volume_up_rounded,
                  _muted ? TW.activarSonido : TW.silenciar,
                  _toggleMute,
                ),
              _iconButton(Icons.ios_share_rounded, TW.compartir, _shareCurrent),
              _iconButton(Icons.close_rounded, TW.cerrar, () => context.go('/wrapped')),
            ],
          ),
        ),
      ]);
    }
    if (_busyMsg != null) {
      stack.add(
        Positioned.fill(
          child: Container(
            color: Colors.black87,
            alignment: Alignment.center,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const CircularProgressIndicator(color: Colors.white),
                const SizedBox(height: 16),
                Text(
                  _busyMsg!,
                  style: const TextStyle(color: Colors.white, fontWeight: FontWeight.w700),
                ),
              ],
            ),
          ),
        ),
      );
    }
    return SizedBox(
      width: kCanvasW,
      height: h,
      child: ClipRect(child: Stack(children: stack)),
    );
  }

  @override
  Widget build(BuildContext context) {
    if (_d == null) {
      return Scaffold(
        backgroundColor: Colors.black,
        body: _error != null
            ? ErrorBox(
                error: _error!,
                onRetry: () {
                  setState(() => _error = null);
                  _load();
                },
              )
            : const Center(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    CircularProgressIndicator(color: Colors.white),
                    SizedBox(height: 16),
                    Text(TW.generando, style: TextStyle(color: Colors.white70)),
                  ],
                ),
              ),
      );
    }
    final bg = _d!.topAlbums.firstOrNull?.image ?? _d!.topArtists.firstOrNull?.image;
    return Scaffold(
      backgroundColor: Colors.black,
      body: Focus(
        focusNode: _focus,
        autofocus: true,
        onKeyEvent: (_, e) {
          if (e is! KeyDownEvent || !_started) return KeyEventResult.ignored;
          switch (e.logicalKey) {
            case LogicalKeyboardKey.arrowRight:
              _next();
            case LogicalKeyboardKey.arrowLeft:
              _prev();
            case LogicalKeyboardKey.space:
              _hold(!_held);
            case LogicalKeyboardKey.escape:
              context.go('/wrapped');
            default:
              return KeyEventResult.ignored;
          }
          return KeyEventResult.handled;
        },
        child: LayoutBuilder(
          builder: (context, c) {
            final narrow = c.maxWidth / c.maxHeight < kCanvasW / kCanvasH;
            // En el móvil el lienzo llena la pantalla (algo más alto que 9:16); al exportar, 9:16 exacto.
            final h = _still || !narrow ? kCanvasH : (kCanvasW * c.maxHeight / c.maxWidth).clamp(kCanvasH, 800.0);
            return Stack(
              children: [
                if (bg != null && !narrow)
                  Positioned.fill(
                    child: ImageFiltered(
                      imageFilter: ui.ImageFilter.blur(sigmaX: 60, sigmaY: 60),
                      child: Image.network(bg, fit: BoxFit.cover, errorBuilder: (_, _, _) => const SizedBox()),
                    ),
                  ),
                if (!narrow) Positioned.fill(child: Container(color: Colors.black.withValues(alpha: 0.55))),
                Center(
                  child: Padding(
                    padding: EdgeInsets.all(narrow ? 0 : 16),
                    child: AspectRatio(
                      aspectRatio: kCanvasW / h,
                      child: ClipRRect(
                        borderRadius: BorderRadius.circular(narrow ? 0 : 18),
                        child: FittedBox(child: _canvas(h)),
                      ),
                    ),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _StartView extends StatelessWidget {
  const _StartView({required this.d, required this.colors, required this.onStart});
  final WrappedData d;
  final List<Color> colors;
  final VoidCallback onStart;

  @override
  Widget build(BuildContext context) => Stack(
    fit: StackFit.expand,
    children: [
      BlobBackground(colors: colors, seed: 1),
      Padding(
        padding: const EdgeInsets.fromLTRB(28, 40, 28, 36),
        child: Column(
          children: [
            Align(
              alignment: Alignment.centerLeft,
              child: IconButton(
                onPressed: () => context.go('/wrapped'),
                icon: const Icon(Icons.arrow_back_rounded, color: Colors.white),
              ),
            ),
            const Spacer(flex: 2),
            Grad('${d.year}', style: wStyle(110, weight: FontWeight.w900, height: 1)),
            const SizedBox(height: 8),
            Text(TW.tuWrapped(d.year), style: wStyle(28, weight: FontWeight.w800)),
            const SizedBox(height: 18),
            Text(d.soundtrack.isEmpty ? TW.sinMusica : TW.subeVolumen, textAlign: TextAlign.center, style: wSub),
            if (!d.isFinal) ...[const SizedBox(height: 10), Pill(TW.vistaPreviaExplica, icon: Icons.schedule_rounded)],
            const Spacer(flex: 3),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                style: FilledButton.styleFrom(
                  backgroundColor: Colors.white,
                  foregroundColor: Colors.black,
                  padding: const EdgeInsets.symmetric(vertical: 16),
                ),
                onPressed: onStart,
                icon: const Icon(Icons.play_arrow_rounded),
                label: const Text(TW.empezar, style: TextStyle(fontWeight: FontWeight.w800, fontSize: 17)),
              ),
            ),
          ],
        ),
      ),
    ],
  );
}
