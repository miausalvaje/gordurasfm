import 'package:flutter_test/flutter_test.dart';
import 'package:gordurasfm/format.dart';

void main() {
  test('números en español', () {
    expect(fmtInt(0), '0');
    expect(fmtInt(999), '999');
    expect(fmtInt(21714), '21.714');
    expect(fmtInt(1234567), '1.234.567');
    expect(fmtInt(-1500), '−1.500');
    expect(fmtDecimal(29.5), '29,5');
    expect(fmtDecimal(3.04), '3');
    expect(fmtDecimal(1234.56), '1.234,6');
  });

  test('duraciones', () {
    expect(fmtDuration(0), '0 min');
    expect(fmtDuration(30 * 1000), '<1 min');
    expect(fmtDuration(45 * 60000), '45 min');
    expect(fmtDuration(125 * 60000), '2 h 5 min');
    expect(fmtDuration(1377 * 3600000), '1.377 h');
    expect(fmtTrackLength(187000), '3:07');
  });

  test('cambio porcentual', () {
    expect(fmtChange(12.4), '+12 %');
    expect(fmtChange(-2.3), '−2 %');
    expect(fmtChange(0.2), '= 0 %');
  });

  test('fechas relativas', () {
    final now = DateTime(2026, 11, 26, 12, 0);
    expect(fmtRelative(now.subtract(const Duration(seconds: 20)), now: now), 'ahora');
    expect(fmtRelative(now.subtract(const Duration(minutes: 5)), now: now), 'hace 5 min');
    expect(fmtRelative(now.subtract(const Duration(hours: 3)), now: now), 'hace 3 h');
    expect(fmtRelative(DateTime(2026, 11, 26, 1, 15), now: now), '01:15');
    expect(fmtRelative(DateTime(2026, 11, 25, 22, 13), now: now), 'ayer, 22:13');
    expect(fmtRelative(DateTime(2026, 11, 23, 22, 13), now: now), 'lun 23 nov, 22:13');
    expect(fmtRelative(DateTime(2025, 3, 2, 9, 5), now: now), '2 mar 2025, 09:05');
  });

  test('ejes de la gráfica', () {
    expect(chartTick('2026-11-23', 'day', 7, 0), 'L');
    expect(chartTick('2026-11-10', 'day', 30, 9), '10');
    expect(chartTick('2026-11-11', 'day', 30, 10), '');
    expect(chartTick('2026-03', 'month', 12, 2), 'mar');
    expect(chartTick('2026-01', 'month', 36, 12), '’26');
    expect(chartLabel('2026-03', 'month'), 'marzo 2026');
  });

  test('iniciales', () {
    expect(initials('Bad Gyal'), 'BG');
    expect(initials('pablo'), 'PA');
    expect(initials('  '), '?');
  });
}
