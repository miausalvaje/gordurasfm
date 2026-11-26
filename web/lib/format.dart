// Formato de números, duraciones y fechas en español (sin depender de intl).

const diasSemana = ['lunes', 'martes', 'miércoles', 'jueves', 'viernes', 'sábado', 'domingo'];
const diasCortos = ['lun', 'mar', 'mié', 'jue', 'vie', 'sáb', 'dom'];
const diasInicial = ['L', 'M', 'X', 'J', 'V', 'S', 'D'];
const meses = [
  'enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', //
  'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre',
];
const mesesCortos = ['ene', 'feb', 'mar', 'abr', 'may', 'jun', 'jul', 'ago', 'sep', 'oct', 'nov', 'dic'];

/// 12345 → «12.345».
String fmtInt(num n) {
  final neg = n < 0;
  final s = n.round().abs().toString();
  final b = StringBuffer();
  for (var i = 0; i < s.length; i++) {
    if (i > 0 && (s.length - i) % 3 == 0) b.write('.');
    b.write(s[i]);
  }
  return '${neg ? '−' : ''}$b';
}

/// 3,456 → «3,5».
String fmtDecimal(double v) {
  final r = (v * 10).round() / 10;
  final s = r == r.roundToDouble() ? fmtInt(r) : '${fmtInt(r.truncate())},${((r.abs() * 10).round() % 10)}';
  return s;
}

/// Tiempo escuchado: «45 min», «12 h 5 min», «1.234 h».
String fmtDuration(int ms) {
  final min = ms ~/ 60000;
  if (min < 1) return ms > 0 ? '<1 min' : '0 min';
  if (min < 60) return '$min min';
  final h = min ~/ 60;
  final m = min % 60;
  if (h < 100) return m == 0 ? '$h h' : '$h h $m min';
  return '${fmtInt(h)} h';
}

/// Duración de una canción: «3:07».
String fmtTrackLength(int ms) {
  final s = ms ~/ 1000;
  return '${s ~/ 60}:${(s % 60).toString().padLeft(2, '0')}';
}

/// «+12 %» / «−5 %».
String fmtChange(double pct) {
  final v = pct.round();
  if (v == 0) return '= 0 %';
  return '${v > 0 ? '+' : '−'}${fmtInt(v.abs())} %';
}

DateTime fromUnix(int s) => DateTime.fromMillisecondsSinceEpoch(s * 1000);

/// «lun 23 nov».
String fmtShortDate(DateTime d) => '${diasCortos[d.weekday - 1]} ${d.day} ${mesesCortos[d.month - 1]}';

/// «23 de noviembre de 2026».
String fmtLongDate(DateTime d) => '${d.day} de ${meses[d.month - 1]} de ${d.year}';

String fmtTime(DateTime d) => '${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';

/// «ahora», «hace 5 min», «hace 3 h», «ayer, 22:13», «lun 23 nov, 22:13».
String fmtRelative(DateTime d, {DateTime? now}) {
  now ??= DateTime.now();
  final diff = now.difference(d);
  if (diff.inMinutes < 1) return 'ahora';
  if (diff.inMinutes < 60) return 'hace ${diff.inMinutes} min';
  final today = DateTime(now.year, now.month, now.day);
  final day = DateTime(d.year, d.month, d.day);
  final days = today.difference(day).inDays;
  if (days == 0) return diff.inHours < 6 ? 'hace ${diff.inHours} h' : fmtTime(d);
  if (days == 1) return 'ayer, ${fmtTime(d)}';
  if (d.year == now.year) return '${fmtShortDate(d)}, ${fmtTime(d)}';
  return '${d.day} ${mesesCortos[d.month - 1]} ${d.year}, ${fmtTime(d)}';
}

/// Etiqueta corta del eje de la gráfica. Devuelve '' si no toca mostrarla (para no amontonar).
String chartTick(String label, String unit, int count, int index) {
  switch (unit) {
    case 'day':
      final d = DateTime.parse(label);
      if (count <= 7) return diasInicial[d.weekday - 1];
      final every = count > 20 ? 5 : 2;
      return (d.day == 1 || d.day % every == 0) ? '${d.day}' : '';
    case 'month':
      final p = label.split('-');
      final m = int.parse(p[1]);
      if (count <= 12) return mesesCortos[m - 1];
      return m == 1 ? '’${p[0].substring(2)}' : '';
    default:
      return count > 8 ? '’${label.substring(2)}' : label;
  }
}

/// Etiqueta completa (tooltip de la gráfica).
String chartLabel(String label, String unit) {
  switch (unit) {
    case 'day':
      return fmtShortDate(DateTime.parse(label));
    case 'month':
      final p = label.split('-');
      return '${meses[int.parse(p[1]) - 1]} ${p[0]}';
    default:
      return label;
  }
}

/// Iniciales para un avatar: «Bad Gyal» → «BG».
String initials(String s) {
  final parts = s.trim().split(RegExp(r'\s+')).where((p) => p.isNotEmpty).toList();
  if (parts.isEmpty) return '?';
  if (parts.length == 1) return parts.first.characters2.toUpperCase();
  return (parts[0].characters1 + parts[1].characters1).toUpperCase();
}

extension on String {
  String get characters1 => isEmpty ? '' : String.fromCharCode(runes.first);
  String get characters2 => runes.take(2).map(String.fromCharCode).join();
}
