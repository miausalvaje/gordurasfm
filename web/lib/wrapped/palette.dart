import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:http/http.dart' as http;
import 'package:material_ui/material_ui.dart';

import '../theme.dart';

/// Colores del Wrapped sacados de las portadas (con los de la marca de reserva).
class WPalette {
  const WPalette(this.colors);
  final List<Color> colors;

  static const fallback = WPalette([C.accent, C.accent2, Color(0xFF7B2FF7), Color(0xFF00B4D8)]);

  Color at(int i) => colors[i % colors.length];

  /// Tres colores para el fondo de una diapositiva.
  List<Color> forSlide(int i) => [at(i), at(i + 1), at(i + 2)];
}

/// Elige colores vivos y distintos entre sí a partir de los píxeles (RGBA) de varias imágenes.
List<Color> pickColors(List<List<int>> rgbaImages, {int max = 4}) {
  final cands = <HSVColor>[];
  for (final px in rgbaImages) {
    for (var i = 0; i + 3 < px.length; i += 4) {
      if (px[i + 3] < 200) continue;
      final hsv = HSVColor.fromColor(Color.fromARGB(255, px[i], px[i + 1], px[i + 2]));
      if (hsv.saturation < 0.25 || hsv.value < 0.25) continue;
      cands.add(hsv);
    }
  }
  cands.sort((a, b) => (b.saturation * b.value).compareTo(a.saturation * a.value));
  final out = <HSVColor>[];
  for (final c in cands) {
    final far = out.every((o) {
      final d = (c.hue - o.hue).abs();
      return math.min(d, 360 - d) > 35;
    });
    if (far) out.add(c);
    if (out.length == max) break;
  }
  // Colores de fondo: saturados pero no chillones.
  return [
    for (final c in out)
      c.withSaturation(math.min(c.saturation, 0.85)).withValue(math.min(math.max(c.value, 0.55), 0.9)).toColor(),
  ];
}

Future<List<int>?> _pixels(String url) async {
  try {
    final r = await http.get(Uri.parse(url));
    if (r.statusCode != 200) return null;
    final codec = await ui.instantiateImageCodec(r.bodyBytes, targetWidth: 20, targetHeight: 20);
    final frame = await codec.getNextFrame();
    final data = await frame.image.toByteData(format: ui.ImageByteFormat.rawRgba);
    return data?.buffer.asUint8List().toList();
  } catch (_) {
    return null;
  }
}

Future<WPalette> paletteFrom(List<String> urls) async {
  final px = <List<int>>[];
  for (final u in urls.take(4)) {
    final p = await _pixels(u);
    if (p != null) px.add(p);
  }
  final colors = pickColors(px);
  if (colors.isEmpty) return WPalette.fallback;
  // Se completa con los de la marca para tener siempre 4.
  for (final c in WPalette.fallback.colors) {
    if (colors.length >= 4) break;
    colors.add(c);
  }
  return WPalette(colors);
}
