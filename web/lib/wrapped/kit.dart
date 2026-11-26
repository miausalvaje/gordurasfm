import 'package:material_ui/material_ui.dart';

import '../format.dart';
import '../theme.dart';

/// Tamaño lógico del lienzo de las diapositivas (9:16). Se exporta a 1080×1920 (×3).
const kCanvasW = 360.0;
const kCanvasH = 640.0;

const displayFont = 'InterDisplay';

TextStyle wStyle(double size, {FontWeight weight = FontWeight.w800, Color color = Colors.white, double? height}) =>
    TextStyle(
      fontFamily: displayFont,
      fontSize: size,
      fontWeight: weight,
      color: color,
      height: height ?? 1.05,
      letterSpacing: size > 40 ? -0.4 : -0.2,
    );

const wSub = TextStyle(fontSize: 15, fontWeight: FontWeight.w600, color: Colors.white70, height: 1.3);

double _interval(double t, double begin, double end) =>
    end <= begin ? (t >= end ? 1 : 0) : ((t - begin) / (end - begin)).clamp(0.0, 1.0);

/// Aparece (opacidad + desplazamiento + escala opcional) en el tramo [begin, end] de la animación de entrada.
class Reveal extends StatelessWidget {
  const Reveal({
    super.key,
    required this.t,
    this.begin = 0,
    this.end = 0.5,
    this.dy = 26,
    this.dx = 0,
    this.scale = 1,
    required this.child,
  });

  final Animation<double> t;
  final double begin;
  final double end;
  final double dy;
  final double dx;

  /// Escala inicial (1 = sin escala).
  final double scale;
  final Widget child;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: t,
    child: child,
    builder: (_, child) {
      final v = Curves.easeOutCubic.transform(_interval(t.value, begin, end));
      final s = scale + (1 - scale) * Curves.easeOutBack.transform(_interval(t.value, begin, end));
      return Opacity(
        opacity: v,
        child: Transform.translate(
          offset: Offset(dx * (1 - v), dy * (1 - v)),
          child: scale == 1 ? child : Transform.scale(scale: s, child: child),
        ),
      );
    },
  );
}

/// Número que cuenta desde 0 hasta [value].
class CountUp extends StatelessWidget {
  const CountUp({
    super.key,
    required this.t,
    required this.value,
    required this.style,
    this.begin = 0.1,
    this.end = 0.8,
  });

  final Animation<double> t;
  final int value;
  final TextStyle style;
  final double begin;
  final double end;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: t,
    builder: (_, _) {
      final v = Curves.easeOutQuart.transform(_interval(t.value, begin, end));
      return Text(fmtInt(value * v), style: style, maxLines: 1);
    },
  );
}

/// Texto con el degradado de la marca.
class Grad extends StatelessWidget {
  const Grad(this.text, {super.key, required this.style, this.colors});
  final String text;
  final TextStyle style;
  final List<Color>? colors;

  @override
  Widget build(BuildContext context) => ShaderMask(
    shaderCallback: (r) => LinearGradient(
      colors: colors ?? const [Color(0xFFFFD166), C.accent2, C.accent],
      begin: Alignment.topLeft,
      end: Alignment.bottomRight,
    ).createShader(r),
    blendMode: BlendMode.srcIn,
    child: Text(text, style: style, maxLines: 1),
  );
}

/// Texto que se encoge para caber (nombres largos).
class FitText extends StatelessWidget {
  const FitText(this.text, {super.key, required this.style, this.maxLines = 2, this.align = TextAlign.center});
  final String text;
  final TextStyle style;
  final int maxLines;
  final TextAlign align;

  @override
  Widget build(BuildContext context) {
    final size = style.fontSize ?? 20;
    // Reducción aproximada según la longitud, para no partir palabras en tres líneas.
    final len = text.length;
    final scale = len <= 12 ? 1.0 : (len <= 20 ? 0.8 : (len <= 32 ? 0.62 : 0.5));
    return Text(
      text,
      textAlign: align,
      maxLines: maxLines,
      overflow: TextOverflow.ellipsis,
      style: style.copyWith(fontSize: size * scale),
    );
  }
}

/// Imagen del Wrapped: portada (cuadrada) o artista (redonda, con collage de portadas si no hay foto).
class WImage extends StatelessWidget {
  const WImage({
    super.key,
    required this.url,
    required this.seed,
    this.covers = const [],
    this.size = 64,
    this.round = false,
    this.radius,
    this.shadow = false,
  });

  final String? url;
  final String seed;
  final List<String> covers;
  final double size;
  final bool round;
  final double? radius;
  final bool shadow;

  Widget _net(String u) =>
      Image.network(u, fit: BoxFit.cover, gaplessPlayback: true, errorBuilder: (_, _, _) => _placeholder());

  Widget _placeholder() {
    final c = C.seeded(seed);
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [c, Color.lerp(c, Colors.black, 0.5)!],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
      ),
      child: Center(
        child: round
            ? Text(
                initials(seed),
                style: TextStyle(fontSize: size * 0.32, fontWeight: FontWeight.w800, color: Colors.white),
              )
            : Icon(Icons.album_rounded, size: size * 0.42, color: Colors.white54),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    Widget img;
    if (url != null) {
      img = _net(url!);
    } else if (covers.length >= 4) {
      img = GridView.count(
        crossAxisCount: 2,
        physics: const NeverScrollableScrollPhysics(),
        padding: EdgeInsets.zero,
        children: [for (final c in covers.take(4)) _net(c)],
      );
    } else if (covers.isNotEmpty) {
      img = _net(covers.first);
    } else {
      img = _placeholder();
    }
    final r = radius ?? size * 0.08;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        shape: round ? BoxShape.circle : BoxShape.rectangle,
        borderRadius: round ? null : BorderRadius.circular(r),
        boxShadow: shadow
            ? [
                BoxShadow(
                  color: Colors.black.withValues(alpha: 0.5),
                  blurRadius: size * 0.18,
                  offset: Offset(0, size * 0.06),
                ),
              ]
            : null,
      ),
      child: round ? ClipOval(child: img) : ClipRRect(borderRadius: BorderRadius.circular(r), child: img),
    );
  }
}

/// Píldora pequeña (etiquetas).
class Pill extends StatelessWidget {
  const Pill(this.text, {super.key, this.color = Colors.white, this.icon});
  final String text;
  final Color color;
  final IconData? icon;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
    decoration: BoxDecoration(
      color: color.withValues(alpha: 0.16),
      borderRadius: BorderRadius.circular(99),
      border: Border.all(color: color.withValues(alpha: 0.4)),
    ),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (icon != null) ...[Icon(icon, size: 15, color: color), const SizedBox(width: 5)],
        Text(
          text,
          style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700, color: color),
        ),
      ],
    ),
  );
}
