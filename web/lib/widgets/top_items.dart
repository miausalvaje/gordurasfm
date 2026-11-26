import 'package:flutter_animate/flutter_animate.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../theme.dart';
import 'common.dart';
import 'cover.dart';

/// Fila de canción: posición, portada, título, artista · álbum y escuchas con barra.
class TrackRow extends StatelessWidget {
  const TrackRow({super.key, required this.item, required this.maxPlays, this.index = 0});
  final TopItem item;
  final int maxPlays;
  final int index;

  @override
  Widget build(BuildContext context) {
    final sub = [item.artist, item.album].whereType<String>().where((s) => s.isNotEmpty).join(' · ');
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Row(
        children: [
          RankText(item.rank),
          const SizedBox(width: 8),
          Cover(url: item.cover, seed: item.album ?? item.title, size: 48),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  item.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                const SizedBox(height: 2),
                Text(
                  sub,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: C.muted, fontSize: 13),
                ),
                const SizedBox(height: 6),
                PlaysBar(value: item.plays, max: maxPlays),
              ],
            ),
          ),
          const SizedBox(width: 12),
          PlaysCount(item.plays),
        ],
      ),
    ).animate().fadeIn(duration: 250.ms, delay: (20 * index).ms);
  }
}

class PlaysCount extends StatelessWidget {
  const PlaysCount(this.plays, {super.key});
  final int plays;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.end,
    children: [
      Text(
        fmtInt(plays),
        style: const TextStyle(fontWeight: FontWeight.w700, fontFeatures: [FontFeature.tabularFigures()]),
      ),
      const Text('escuchas', style: TextStyle(color: C.muted, fontSize: 11)),
    ],
  );
}

class PlaysBar extends StatelessWidget {
  const PlaysBar({super.key, required this.value, required this.max});
  final int value;
  final int max;

  @override
  Widget build(BuildContext context) => ClipRRect(
    borderRadius: BorderRadius.circular(3),
    child: SizedBox(
      height: 4,
      child: Stack(
        children: [
          Container(color: C.surface2),
          FractionallySizedBox(
            widthFactor: max <= 0 ? 0 : (value / max).clamp(0.02, 1.0),
            child: Container(decoration: const BoxDecoration(gradient: C.gradient)),
          ),
        ],
      ),
    ),
  );
}

/// Tarjeta de álbum: portada grande con posición, título, artista y escuchas.
class AlbumCard extends StatelessWidget {
  const AlbumCard({super.key, required this.item, this.width = 150, this.index = 0});
  final TopItem item;
  final double width;
  final int index;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: width,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Stack(
          children: [
            Cover(url: item.cover, seed: item.title, size: width, radius: 14),
            Positioned(left: 8, top: 8, child: RankBadge(item.rank)),
          ],
        ),
        const SizedBox(height: 8),
        Text(
          item.title,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(fontWeight: FontWeight.w600),
        ),
        Text(
          item.artist ?? '',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: C.muted, fontSize: 13),
        ),
        const SizedBox(height: 2),
        Text(
          '${fmtInt(item.plays)} escuchas',
          style: const TextStyle(color: C.accent2, fontSize: 12, fontWeight: FontWeight.w600),
        ),
      ],
    ),
  ).animate().fadeIn(duration: 300.ms, delay: (30 * index).ms).slideY(begin: 0.06, end: 0);
}

/// Artista en círculo (para listas horizontales).
class ArtistBubble extends StatelessWidget {
  const ArtistBubble({super.key, required this.item, this.size = 110, this.index = 0});
  final TopItem item;
  final double size;
  final int index;

  @override
  Widget build(BuildContext context) =>
      SizedBox(
            width: size,
            child: Column(
              children: [
                Stack(
                  clipBehavior: Clip.none,
                  children: [
                    ArtistAvatar(name: item.title, image: item.cover, covers: item.covers, size: size),
                    Positioned(left: 0, top: 0, child: RankBadge(item.rank)),
                  ],
                ),
                const SizedBox(height: 8),
                Text(
                  item.title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textAlign: TextAlign.center,
                  style: const TextStyle(fontWeight: FontWeight.w600),
                ),
                Text('${fmtInt(item.plays)} escuchas', style: const TextStyle(color: C.muted, fontSize: 12)),
              ],
            ),
          )
          .animate()
          .fadeIn(duration: 300.ms, delay: (30 * index).ms)
          .scale(begin: const Offset(0.94, 0.94), end: const Offset(1, 1));
}

/// Fila de artista (para listas verticales).
class ArtistRow extends StatelessWidget {
  const ArtistRow({super.key, required this.item, required this.maxPlays, this.index = 0});
  final TopItem item;
  final int maxPlays;
  final int index;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 6),
    child: Row(
      children: [
        RankText(item.rank),
        const SizedBox(width: 8),
        ArtistAvatar(name: item.title, image: item.cover, covers: item.covers, size: 48),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                item.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 8),
              PlaysBar(value: item.plays, max: maxPlays),
            ],
          ),
        ),
        const SizedBox(width: 12),
        PlaysCount(item.plays),
      ],
    ),
  ).animate().fadeIn(duration: 250.ms, delay: (20 * index).ms);
}

/// Fila de álbum (lista vertical del top completo en móvil).
class AlbumRow extends StatelessWidget {
  const AlbumRow({super.key, required this.item, required this.maxPlays, this.index = 0});
  final TopItem item;
  final int maxPlays;
  final int index;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 6),
    child: Row(
      children: [
        RankText(item.rank),
        const SizedBox(width: 8),
        Cover(url: item.cover, seed: item.title, size: 56),
        const SizedBox(width: 12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                item.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(fontWeight: FontWeight.w600),
              ),
              const SizedBox(height: 2),
              Text(
                item.artist ?? '',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(color: C.muted, fontSize: 13),
              ),
              const SizedBox(height: 6),
              PlaysBar(value: item.plays, max: maxPlays),
            ],
          ),
        ),
        const SizedBox(width: 12),
        PlaysCount(item.plays),
      ],
    ),
  ).animate().fadeIn(duration: 250.ms, delay: (20 * index).ms);
}
