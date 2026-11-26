import 'package:go_router/go_router.dart';
import 'package:material_ui/material_ui.dart';

import '../api.dart';
import '../format.dart';
import '../l10n/wrapped_es.dart';
import '../session.dart';
import '../theme.dart';
import '../widgets/common.dart';

/// Lista de Wrapped por año: el del año en curso (vista previa) y los definitivos de años anteriores.
class WrappedListScreen extends StatefulWidget {
  const WrappedListScreen({super.key});

  @override
  State<WrappedListScreen> createState() => _WrappedListScreenState();
}

class _WrappedListScreenState extends State<WrappedListScreen> {
  List<dynamic>? _years;
  Object? _error;
  int? _busyYear;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_years == null && _error == null) _load();
  }

  Future<void> _load() async {
    try {
      final r = await context.api.get('/api/v1/wrapped');
      if (mounted) setState(() => _years = r['years'] as List);
    } catch (e) {
      if (mounted) setState(() => _error = e);
    }
  }

  Future<void> _regenerate(int year) async {
    setState(() => _busyYear = year);
    try {
      await context.api.send('POST', '/api/v1/wrapped/$year/regenerate');
      if (mounted) context.go('/wrapped/$year');
    } on ApiException catch (e) {
      if (mounted) toast(context, e.message, error: true);
    } finally {
      if (mounted) setState(() => _busyYear = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    final years = _years;
    if (years == null) return _error != null ? ErrorBox(error: _error!, onRetry: _load) : const Loading();
    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 40),
      children: [
        ContentWidth(
          max: 820,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text(
                TW.titulo,
                style: TextStyle(fontFamily: 'InterDisplay', fontSize: 28, fontWeight: FontWeight.w900),
              ),
              const SizedBox(height: 4),
              const Text(TW.explica, style: TextStyle(color: C.muted)),
              const SizedBox(height: 16),
              if (years.isEmpty) const Text(TW.sinAnios, style: TextStyle(color: C.muted)),
              for (final y in years) ...[
                _YearCard(
                  data: y as Map<String, dynamic>,
                  busy: _busyYear == y['year'],
                  onOpen: () => context.go('/wrapped/${y['year']}'),
                  onRegenerate: () => _regenerate(y['year'] as int),
                ),
                const SizedBox(height: 12),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

class _YearCard extends StatelessWidget {
  const _YearCard({required this.data, required this.busy, required this.onOpen, required this.onRegenerate});
  final Map<String, dynamic> data;
  final bool busy;
  final VoidCallback onOpen;
  final VoidCallback onRegenerate;

  @override
  Widget build(BuildContext context) {
    final year = data['year'] as int;
    final w = data['wrapped'] as Map<String, dynamic>?;
    final current = data['current'] == true;
    final outdated = w?['outdated'] == true;
    final status = current ? TW.vistaPrevia : (w == null ? TW.sinGenerar : TW.definitivo);
    final hue = (year * 47) % 360;
    final c1 = HSLColor.fromAHSL(1, hue.toDouble(), 0.55, 0.32).toColor();
    final c2 = HSLColor.fromAHSL(1, (hue + 50) % 360, 0.6, 0.38).toColor();
    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(20),
        onTap: onOpen,
        child: Ink(
          padding: const EdgeInsets.all(18),
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(20),
            gradient: LinearGradient(colors: [c1, c2], begin: Alignment.topLeft, end: Alignment.bottomRight),
          ),
          child: Row(
            children: [
              // Ancho fijo: así el texto de al lado queda en columna en todas las tarjetas.
              SizedBox(
                width: 116,
                child: Text(
                  '$year',
                  style: const TextStyle(
                    fontFamily: 'InterDisplay',
                    fontSize: 44,
                    fontWeight: FontWeight.w900,
                    height: 1,
                  ),
                ),
              ),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      TW.escuchasAnio(fmtInt(data['plays'] as int)),
                      style: const TextStyle(fontWeight: FontWeight.w700, fontSize: 15),
                    ),
                    const SizedBox(height: 2),
                    Text(status, style: const TextStyle(color: Colors.white70, fontSize: 13)),
                    if (outdated)
                      Padding(
                        padding: const EdgeInsets.only(top: 6),
                        child: Row(
                          children: [
                            const Icon(Icons.info_outline_rounded, size: 15, color: Colors.white),
                            const SizedBox(width: 5),
                            const Flexible(
                              child: Text(TW.desactualizado, style: TextStyle(fontSize: 12.5, color: Colors.white)),
                            ),
                            const SizedBox(width: 6),
                            busy
                                ? const SizedBox(
                                    width: 16,
                                    height: 16,
                                    child: CircularProgressIndicator(strokeWidth: 2, color: Colors.white),
                                  )
                                : TextButton(
                                    style: TextButton.styleFrom(
                                      foregroundColor: Colors.white,
                                      padding: const EdgeInsets.symmetric(horizontal: 8),
                                      minimumSize: const Size(0, 30),
                                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                                    ),
                                    onPressed: onRegenerate,
                                    child: const Text(TW.rehacer, style: TextStyle(fontWeight: FontWeight.w800)),
                                  ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
              const Icon(Icons.play_circle_fill_rounded, size: 40, color: Colors.white),
            ],
          ),
        ),
      ),
    );
  }
}
