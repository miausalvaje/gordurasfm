import 'package:material_ui/material_ui.dart';

/// Colores de GordurasFM.
class C {
  C._();
  static const bg = Color(0xFF0E0E12);
  static const surface = Color(0xFF17171D);
  static const surface2 = Color(0xFF212129);
  static const line = Color(0xFF2C2C36);
  static const text = Color(0xFFF4F4F6);
  static const muted = Color(0xFF9A9AA8);
  static const accent = Color(0xFFFF4D6D);
  static const accent2 = Color(0xFFFF9E3D);
  static const ok = Color(0xFF3DDC97);
  static const bad = Color(0xFFFF6B6B);

  static const gradient = LinearGradient(
    colors: [accent, accent2],
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
  );

  /// Color estable a partir de un texto (para portadas que faltan).
  static Color seeded(String s) {
    var h = 0;
    for (final c in s.codeUnits) {
      h = (h * 31 + c) & 0x7fffffff;
    }
    return HSLColor.fromAHSL(1, (h % 360).toDouble(), 0.55, 0.42).toColor();
  }
}

ThemeData buildTheme() {
  final scheme = ColorScheme.fromSeed(seedColor: C.accent, brightness: Brightness.dark).copyWith(
    primary: C.accent,
    onPrimary: Colors.white,
    primaryContainer: const Color(0xFF5A1A2A),
    onPrimaryContainer: Colors.white,
    secondaryContainer: const Color(0xFF4A2A14),
    onSecondaryContainer: Colors.white,
    secondary: C.accent2,
    surface: C.surface,
    onSurface: C.text,
    surfaceContainerLowest: C.bg,
    surfaceContainerLow: C.surface,
    surfaceContainer: C.surface,
    surfaceContainerHigh: C.surface2,
    surfaceContainerHighest: C.surface2,
    outline: C.line,
    outlineVariant: C.line,
  );
  final base = ThemeData(colorScheme: scheme, useMaterial3: true, fontFamily: 'Inter', brightness: Brightness.dark);
  return base.copyWith(
    scaffoldBackgroundColor: C.bg,
    textTheme: base.textTheme.apply(bodyColor: C.text, displayColor: C.text),
    appBarTheme: const AppBarTheme(
      backgroundColor: C.bg,
      surfaceTintColor: Colors.transparent,
      elevation: 0,
      scrolledUnderElevation: 0,
      centerTitle: false,
    ),
    cardTheme: CardThemeData(
      color: C.surface,
      elevation: 0,
      margin: EdgeInsets.zero,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(18)),
    ),
    chipTheme: base.chipTheme.copyWith(
      backgroundColor: C.surface,
      selectedColor: C.accent,
      side: const BorderSide(color: C.line),
      labelStyle: const TextStyle(fontWeight: FontWeight.w600, fontSize: 13),
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      showCheckmark: false,
    ),
    navigationBarTheme: NavigationBarThemeData(
      backgroundColor: C.surface,
      indicatorColor: C.accent.withValues(alpha: 0.18),
      surfaceTintColor: Colors.transparent,
      labelTextStyle: WidgetStateProperty.resolveWith(
        (s) => s.contains(WidgetState.selected)
            ? const TextStyle(fontSize: 12, fontWeight: FontWeight.w700, color: C.text)
            : const TextStyle(fontSize: 12, fontWeight: FontWeight.w500, color: C.muted),
      ),
      iconTheme: WidgetStateProperty.resolveWith(
        (s) => IconThemeData(color: s.contains(WidgetState.selected) ? C.accent : C.muted),
      ),
    ),
    navigationRailTheme: NavigationRailThemeData(
      backgroundColor: C.bg,
      indicatorColor: C.accent.withValues(alpha: 0.18),
      selectedIconTheme: const IconThemeData(color: C.accent),
      unselectedIconTheme: const IconThemeData(color: C.muted),
      selectedLabelTextStyle: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700, color: C.text),
      unselectedLabelTextStyle: const TextStyle(fontSize: 12, fontWeight: FontWeight.w500, color: C.muted),
    ),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: C.surface2,
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(12), borderSide: BorderSide.none),
    ),
    snackBarTheme: const SnackBarThemeData(behavior: SnackBarBehavior.floating),
    dividerTheme: const DividerThemeData(color: C.line, space: 1),
  );
}
