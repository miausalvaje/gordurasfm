import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../l10n/es.dart';
import '../theme.dart';

/// Ancho máximo del contenido en pantallas grandes.
class ContentWidth extends StatelessWidget {
  const ContentWidth({super.key, required this.child, this.max = 1120});
  final Widget child;
  final double max;

  @override
  Widget build(BuildContext context) => Align(
    alignment: Alignment.topCenter,
    child: ConstrainedBox(
      constraints: BoxConstraints(maxWidth: max),
      child: SizedBox(width: double.infinity, child: child),
    ),
  );
}

class SectionHeader extends StatelessWidget {
  const SectionHeader({super.key, required this.title, this.action, this.onAction});
  final String title;
  final String? action;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: 28, bottom: 12),
    child: Row(
      children: [
        Expanded(
          child: Text(title, style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w700)),
        ),
        if (action != null)
          TextButton(
            onPressed: onAction,
            style: TextButton.styleFrom(
              padding: const EdgeInsets.only(left: 10),
              minimumSize: const Size(0, 36),
              tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(action!, style: const TextStyle(fontWeight: FontWeight.w600)),
                const Icon(Icons.chevron_right_rounded, size: 20),
              ],
            ),
          ),
      ],
    ),
  );
}

/// Selector de periodo (chips desplazables).
class PeriodChips extends StatelessWidget {
  const PeriodChips({super.key, required this.value, required this.onChanged});
  final String value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) => SizedBox(
    height: 40,
    child: ListView(
      scrollDirection: Axis.horizontal,
      clipBehavior: Clip.none,
      children: [
        for (final e in T.periodos.entries)
          Padding(
            padding: const EdgeInsets.only(right: 8),
            child: ChoiceChip(
              label: Text(e.value),
              selected: e.key == value,
              labelStyle: TextStyle(fontWeight: FontWeight.w600, color: e.key == value ? Colors.white : C.text),
              onSelected: (_) => onChanged(e.key),
            ),
          ),
      ],
    ),
  );
}

class ErrorBox extends StatelessWidget {
  const ErrorBox({super.key, required this.error, this.onRetry});
  final Object error;
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(24),
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const Icon(Icons.cloud_off_rounded, size: 40, color: C.muted),
        const SizedBox(height: 12),
        Text(error is ApiException ? error.toString() : T.errorGenerico, textAlign: TextAlign.center),
        if (onRetry != null) ...[
          const SizedBox(height: 12),
          OutlinedButton(onPressed: onRetry, child: const Text(T.reintentarCarga)),
        ],
      ],
    ),
  );
}

class Loading extends StatelessWidget {
  const Loading({super.key});

  @override
  Widget build(BuildContext context) => const Center(
    child: Padding(padding: EdgeInsets.all(32), child: CircularProgressIndicator()),
  );
}

/// Número de posición en un top.
class RankText extends StatelessWidget {
  const RankText(this.rank, {super.key, this.width = 28});
  final int rank;
  final double width;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: width,
    child: Text(
      '$rank',
      textAlign: TextAlign.center,
      style: TextStyle(
        fontWeight: FontWeight.w800,
        fontSize: 15,
        color: rank <= 3 ? C.accent : C.muted,
        fontFeatures: const [FontFeature.tabularFigures()],
      ),
    ),
  );
}

/// Insignia de posición sobre una portada.
class RankBadge extends StatelessWidget {
  const RankBadge(this.rank, {super.key});
  final int rank;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
    decoration: BoxDecoration(
      gradient: rank <= 3 ? C.gradient : null,
      color: rank <= 3 ? null : Colors.black.withValues(alpha: 0.7),
      borderRadius: BorderRadius.circular(10),
    ),
    child: Text(
      '#$rank',
      style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w800, color: Colors.white),
    ),
  );
}

/// Texto con degradado (logo).
class GradientText extends StatelessWidget {
  const GradientText(this.text, {super.key, required this.style});
  final String text;
  final TextStyle style;

  @override
  Widget build(BuildContext context) => ShaderMask(
    shaderCallback: (r) => C.gradient.createShader(r),
    blendMode: BlendMode.srcIn,
    child: Text(text, style: style),
  );
}

class Logo extends StatelessWidget {
  const Logo({super.key, this.size = 22});
  final double size;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Container(
        width: size * 1.3,
        height: size * 1.3,
        decoration: const BoxDecoration(gradient: C.gradient, shape: BoxShape.circle),
        child: Icon(Icons.graphic_eq_rounded, size: size * 0.85, color: Colors.white),
      ),
      SizedBox(width: size * 0.45),
      Text.rich(
        TextSpan(
          children: [
            const TextSpan(text: 'Gorduras'),
            const TextSpan(
              text: 'FM',
              style: TextStyle(color: C.accent),
            ),
          ],
        ),
        style: TextStyle(fontFamily: 'InterDisplay', fontWeight: FontWeight.w900, fontSize: size, letterSpacing: -0.5),
      ),
    ],
  );
}

/// Tarjeta de bloque con título opcional.
class Panel extends StatelessWidget {
  const Panel({super.key, required this.child, this.title, this.subtitle, this.padding = const EdgeInsets.all(18)});
  final Widget child;
  final String? title;
  final String? subtitle;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: padding,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (title != null) Text(title!, style: const TextStyle(fontSize: 17, fontWeight: FontWeight.w700)),
          if (subtitle != null) ...[
            const SizedBox(height: 6),
            Text(subtitle!, style: const TextStyle(color: C.muted, fontSize: 13.5, height: 1.4)),
          ],
          if (title != null || subtitle != null) const SizedBox(height: 14),
          child,
        ],
      ),
    ),
  );
}

void toast(BuildContext context, String msg, {bool error = false}) {
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(msg), backgroundColor: error ? C.bad.withValues(alpha: 0.95) : C.surface2));
}
