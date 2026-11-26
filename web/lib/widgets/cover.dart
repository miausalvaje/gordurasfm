import 'package:material_ui/material_ui.dart';

import '../theme.dart';

/// Portada de álbum con hueco de color si falta.
class Cover extends StatelessWidget {
  const Cover({super.key, required this.url, required this.seed, this.size = 56, this.radius = 8});

  final String? url;

  /// Texto para elegir el color del hueco (título del álbum).
  final String seed;
  final double size;
  final double radius;

  static String sized(String url, double px) {
    final want = px <= 64 ? 150 : (px <= 160 ? 300 : 600);
    return '$url?size=$want';
  }

  @override
  Widget build(BuildContext context) {
    final placeholder = _Placeholder(seed: seed, size: size);
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: SizedBox(
        width: size,
        height: size,
        child: url == null
            ? placeholder
            : Image.network(
                sized(url!, size),
                fit: BoxFit.cover,
                gaplessPlayback: true,
                errorBuilder: (_, _, _) => placeholder,
                frameBuilder: (_, child, frame, sync) =>
                    sync || frame != null ? child : Stack(fit: StackFit.expand, children: [placeholder, child]),
              ),
      ),
    );
  }
}

class _Placeholder extends StatelessWidget {
  const _Placeholder({required this.seed, required this.size});
  final String seed;
  final double size;

  @override
  Widget build(BuildContext context) {
    final c = C.seeded(seed);
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          colors: [c, Color.lerp(c, Colors.black, 0.45)!],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
      ),
      child: Center(
        child: Icon(Icons.album_rounded, size: size * 0.42, color: Colors.white.withValues(alpha: 0.55)),
      ),
    );
  }
}

/// Imagen de artista: la de Navidrome, si no un collage de sus portadas, si no las iniciales.
class ArtistAvatar extends StatelessWidget {
  const ArtistAvatar({super.key, required this.name, this.image, this.covers = const [], this.size = 56});

  final String name;
  final String? image;
  final List<String> covers;
  final double size;

  @override
  Widget build(BuildContext context) {
    final fallback = _collageOrInitials();
    return ClipOval(
      child: SizedBox(
        width: size,
        height: size,
        child: image == null
            ? fallback
            : Image.network(Cover.sized(image!, size), fit: BoxFit.cover, errorBuilder: (_, _, _) => fallback),
      ),
    );
  }

  Widget _collageOrInitials() {
    if (covers.isEmpty) return _initials();
    if (covers.length < 4) {
      return Image.network(Cover.sized(covers.first, size), fit: BoxFit.cover, errorBuilder: (_, _, _) => _initials());
    }
    final half = size / 2;
    return GridView.count(
      crossAxisCount: 2,
      physics: const NeverScrollableScrollPhysics(),
      padding: EdgeInsets.zero,
      children: [
        for (final c in covers.take(4))
          Image.network(
            Cover.sized(c, half),
            fit: BoxFit.cover,
            errorBuilder: (_, _, _) => Container(color: C.seeded(c)),
          ),
      ],
    );
  }

  Widget _initials() {
    final c = C.seeded(name);
    final parts = name.trim().split(RegExp(r'\s+')).where((p) => p.isNotEmpty).toList();
    final txt = parts.isEmpty
        ? '?'
        : (parts.length == 1
                  ? String.fromCharCodes(parts.first.runes.take(2))
                  : String.fromCharCode(parts[0].runes.first) + String.fromCharCode(parts[1].runes.first))
              .toUpperCase();
    return DecoratedBox(
      decoration: BoxDecoration(gradient: LinearGradient(colors: [c, Color.lerp(c, Colors.black, 0.5)!])),
      child: Center(
        child: Text(
          txt,
          style: TextStyle(fontSize: size * 0.34, fontWeight: FontWeight.w700, color: Colors.white),
        ),
      ),
    );
  }
}
