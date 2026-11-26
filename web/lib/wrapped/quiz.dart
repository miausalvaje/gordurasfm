import 'dart:async';
import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../l10n/wrapped_es.dart';
import 'kit.dart';
import 'model.dart';
import 'slides.dart';

enum _Phase { choosing, suspense, revealed }

/// Prueba: elegir el más escuchado entre los 5 primeros (mezclados), confirmar, suspense y revelación
/// con el ranking real.
class QuizSlide extends StatefulWidget {
  const QuizSlide(
    this.a, {
    super.key,
    required this.kind,
    required this.items,
    this.answered,
    required this.onAnswer,
    required this.onRevealed,
  });

  final SlideArgs a;
  final String kind;
  final List<WItem> items;

  /// Posición elegida si ya se ha contestado en esta visualización (se enseña directamente la revelación).
  final int? answered;
  final void Function(int chosenRank) onAnswer;

  /// Se llama al terminar la revelación (el reproductor vuelve a avanzar solo).
  final VoidCallback onRevealed;

  @override
  State<QuizSlide> createState() => _QuizSlideState();
}

class _QuizSlideState extends State<QuizSlide> with TickerProviderStateMixin {
  late final List<WItem> _options = [...widget.items]..shuffle(math.Random());
  int? _selected;
  _Phase _phase = _Phase.choosing;
  late final AnimationController _drum = AnimationController(vsync: this, duration: const Duration(milliseconds: 380));
  late final AnimationController _reveal = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1600),
  );
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    if (widget.answered != null || widget.a.still) {
      _selected = widget.answered;
      _phase = _Phase.revealed;
      _reveal.value = 1;
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    _drum.dispose();
    _reveal.dispose();
    super.dispose();
  }

  void _confirm() {
    final rank = _selected;
    if (rank == null || _phase != _Phase.choosing) return;
    widget.onAnswer(rank);
    setState(() => _phase = _Phase.suspense);
    _drum.repeat(reverse: true);
    _timer = Timer(const Duration(milliseconds: 1500), () {
      if (!mounted) return;
      _drum.stop();
      setState(() => _phase = _Phase.revealed);
      _reveal.forward(from: 0).whenComplete(() {
        if (mounted) widget.onRevealed();
      });
    });
  }

  @override
  Widget build(BuildContext context) {
    final a = widget.a;
    return Stack(
      children: [
        SlidePad(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Reveal(
                t: a.t,
                end: 0.35,
                child: Text(
                  TW.pregunta[widget.kind]!,
                  textAlign: TextAlign.center,
                  style: wStyle(28, weight: FontWeight.w900, height: 1.12),
                ),
              ),
              const SizedBox(height: 6),
              if (_phase == _Phase.choosing)
                Reveal(
                  t: a.t,
                  begin: 0.15,
                  end: 0.45,
                  child: const Text(TW.elige, textAlign: TextAlign.center, style: wSub),
                ),
              const Spacer(),
              Expanded(
                flex: 12,
                child: FittedBox(
                  fit: BoxFit.scaleDown,
                  child: SizedBox(width: kCanvasW - 52, child: _phase == _Phase.revealed ? _ranking() : _choices()),
                ),
              ),
              const Spacer(),
              if (_phase != _Phase.revealed)
                AnimatedOpacity(
                  opacity: _phase == _Phase.choosing ? 1 : 0,
                  duration: const Duration(milliseconds: 200),
                  child: FilledButton(
                    style: FilledButton.styleFrom(
                      backgroundColor: Colors.white,
                      foregroundColor: Colors.black,
                      disabledBackgroundColor: Colors.white24,
                      disabledForegroundColor: Colors.white54,
                      padding: const EdgeInsets.symmetric(vertical: 14),
                    ),
                    onPressed: _selected == null ? null : _confirm,
                    child: const Text(TW.confirmar, style: TextStyle(fontWeight: FontWeight.w800, fontSize: 16)),
                  ),
                )
              else if (!a.still)
                const Center(
                  child: Text(TW.tocaSeguir, style: TextStyle(fontSize: 12.5, color: Colors.white60)),
                )
              else
                Footer(year: a.d.year),
            ],
          ),
        ),
        if (_phase == _Phase.suspense) Positioned.fill(child: _suspense()),
      ],
    );
  }

  Widget _choices() {
    final a = widget.a;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 0; i < _options.length; i++)
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: Reveal(
              t: a.t,
              begin: 0.2 + i * 0.08,
              end: 0.55 + i * 0.08,
              dy: 18,
              child: _Option(
                item: _options[i],
                selected: identical(_options[i], _selectedItem),
                onTap: _phase == _Phase.choosing
                    ? () => setState(() {
                        _selected = _options[i].rank;
                        _selectedItem = _options[i];
                      })
                    : null,
              ),
            ),
          ),
      ],
    );
  }

  WItem? _selectedItem;

  Widget _suspense() => AnimatedBuilder(
    animation: _drum,
    builder: (_, _) => Container(
      color: Colors.black.withValues(alpha: 0.72),
      alignment: Alignment.center,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Transform.scale(
            scale: 1 + _drum.value * 0.18,
            child: Container(
              width: 96,
              height: 96,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: const LinearGradient(colors: [Color(0xFFFFD166), Color(0xFFFF4D6D)]),
                boxShadow: [
                  BoxShadow(
                    color: const Color(0xFFFF4D6D).withValues(alpha: 0.4 + _drum.value * 0.4),
                    blurRadius: 30 + _drum.value * 30,
                  ),
                ],
              ),
              child: const Icon(Icons.music_note_rounded, size: 48, color: Colors.white),
            ),
          ),
          const SizedBox(height: 20),
          Text(TW.redoble, style: wStyle(22, weight: FontWeight.w800)),
        ],
      ),
    ),
  );

  Widget _ranking() {
    final items = [...widget.items]..sort((x, y) => x.rank.compareTo(y.rank));
    final winner = items.first;
    final chosen = _selected;
    final correct = chosen != null && chosen == 1;
    final ok = const Color(0xFF7CF5B5);
    final bad = const Color(0xFFFFB4A2);
    return AnimatedBuilder(
      animation: _reveal,
      builder: (_, _) {
        double seg(double b, double e) => Curves.easeOutBack.transform(((_reveal.value - b) / (e - b)).clamp(0.0, 1.0));
        return Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (chosen != null)
              Transform.scale(
                scale: seg(0, 0.35),
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                  decoration: BoxDecoration(
                    color: (correct ? ok : bad).withValues(alpha: 0.18),
                    borderRadius: BorderRadius.circular(99),
                    border: Border.all(color: correct ? ok : bad, width: 1.5),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(correct ? Icons.check_circle_rounded : Icons.cancel_rounded, color: correct ? ok : bad),
                      const SizedBox(width: 8),
                      Flexible(
                        child: Text(
                          correct ? TW.acierto : TW.fallo(winner.title),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(fontSize: 16, fontWeight: FontWeight.w800, color: correct ? ok : bad),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            const SizedBox(height: 16),
            for (var i = 0; i < items.length; i++)
              Opacity(
                opacity: seg(0.2 + i * 0.12, 0.5 + i * 0.12).clamp(0.0, 1.0),
                child: Transform.translate(
                  offset: Offset(0, 20 * (1 - seg(0.2 + i * 0.12, 0.5 + i * 0.12))),
                  child: Padding(
                    padding: const EdgeInsets.only(bottom: 8),
                    child: _RankRow(item: items[i], highlight: items[i].rank == 1, chosen: chosen == items[i].rank),
                  ),
                ),
              ),
          ],
        );
      },
    );
  }
}

class _Option extends StatelessWidget {
  const _Option({required this.item, required this.selected, this.onTap});
  final WItem item;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) => Material(
    color: Colors.transparent,
    child: InkWell(
      borderRadius: BorderRadius.circular(16),
      onTap: onTap,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 180),
        padding: const EdgeInsets.all(8),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(16),
          color: selected ? Colors.white.withValues(alpha: 0.22) : Colors.white.withValues(alpha: 0.08),
          border: Border.all(
            color: selected ? Colors.white : Colors.white.withValues(alpha: 0.14),
            width: selected ? 2 : 1,
          ),
        ),
        child: Row(
          children: [
            WImage(url: item.image, covers: item.covers, seed: item.title, size: 50, round: item.round),
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
                  if (item.subtitle != null)
                    Text(
                      item.subtitle!,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(fontSize: 12.5, color: Colors.white70),
                    ),
                ],
              ),
            ),
            AnimatedOpacity(
              opacity: selected ? 1 : 0,
              duration: const Duration(milliseconds: 180),
              child: const Icon(Icons.check_circle_rounded, color: Colors.white),
            ),
          ],
        ),
      ),
    ),
  );
}

class _RankRow extends StatelessWidget {
  const _RankRow({required this.item, required this.highlight, required this.chosen});
  final WItem item;
  final bool highlight;
  final bool chosen;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
    decoration: BoxDecoration(
      borderRadius: BorderRadius.circular(14),
      color: highlight ? Colors.white.withValues(alpha: 0.18) : Colors.white.withValues(alpha: 0.06),
      border: Border.all(color: chosen ? Colors.white : Colors.transparent, width: 1.5),
    ),
    child: Row(
      children: [
        SizedBox(
          width: 26,
          child: highlight
              ? Grad('${item.rank}', style: wStyle(22, weight: FontWeight.w900))
              : Text(
                  '${item.rank}',
                  style: wStyle(19, weight: FontWeight.w900, color: Colors.white70),
                ),
        ),
        WImage(url: item.image, covers: item.covers, seed: item.title, size: 40, round: item.round),
        const SizedBox(width: 10),
        Expanded(
          child: Text(
            item.title,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 15,
              fontWeight: highlight ? FontWeight.w900 : FontWeight.w700,
              color: Colors.white,
            ),
          ),
        ),
        const SizedBox(width: 8),
        Text(
          TW.escuchas(item.plays),
          style: const TextStyle(fontSize: 12.5, fontWeight: FontWeight.w700, color: Color(0xFFFFC27A)),
        ),
      ],
    ),
  );
}
