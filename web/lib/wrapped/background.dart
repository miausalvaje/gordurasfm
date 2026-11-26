import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

/// Fondo animado: manchas de color difuminadas que se mueven despacio sobre un degradado oscuro.
class BlobBackground extends StatefulWidget {
  const BlobBackground({super.key, required this.colors, this.seed = 0, this.still = false});

  final List<Color> colors;
  final int seed;

  /// Sin movimiento (para exportar a imagen).
  final bool still;

  @override
  State<BlobBackground> createState() => _BlobBackgroundState();
}

class _BlobBackgroundState extends State<BlobBackground> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(seconds: 18));

  @override
  void initState() {
    super.initState();
    if (!widget.still) _c.repeat();
  }

  @override
  void didUpdateWidget(BlobBackground old) {
    super.didUpdateWidget(old);
    if (widget.still && _c.isAnimating) _c.stop();
    if (!widget.still && !_c.isAnimating) _c.repeat();
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RepaintBoundary(
    child: AnimatedBuilder(
      animation: _c,
      builder: (_, _) =>
          CustomPaint(painter: _BlobPainter(widget.colors, _c.value, widget.seed), child: const SizedBox.expand()),
    ),
  );
}

class _BlobPainter extends CustomPainter {
  _BlobPainter(this.colors, this.t, this.seed);
  final List<Color> colors;
  final double t;
  final int seed;

  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    final base = Color.lerp(colors.first, Colors.black, 0.82)!;
    canvas.drawRect(
      rect,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [base, const Color(0xFF07070A)],
        ).createShader(rect),
    );
    final rnd = math.Random(seed);
    final a = t * 2 * math.pi;
    for (var i = 0; i < 4; i++) {
      final c = colors[i % colors.length];
      final px = rnd.nextDouble();
      final py = rnd.nextDouble();
      final ph = rnd.nextDouble() * 2 * math.pi;
      final r = size.width * (0.45 + rnd.nextDouble() * 0.3);
      final center = Offset(
        size.width * (px + 0.18 * math.sin(a + ph)),
        size.height * (py + 0.12 * math.cos(a * (i.isEven ? 1 : -1) + ph)),
      );
      canvas.drawCircle(
        center,
        r,
        Paint()
          ..shader = RadialGradient(colors: [c.withValues(alpha: 0.55), c.withValues(alpha: 0)])
              .createShader(Rect.fromCircle(center: center, radius: r)),
      );
    }
    // Viñeta para que el texto se lea bien.
    canvas.drawRect(
      rect,
      Paint()
        ..shader = RadialGradient(
          radius: 1.1,
          colors: [Colors.transparent, Colors.black.withValues(alpha: 0.45)],
        ).createShader(rect),
    );
  }

  @override
  bool shouldRepaint(_BlobPainter o) => o.t != t || o.colors != colors || o.seed != seed;
}
