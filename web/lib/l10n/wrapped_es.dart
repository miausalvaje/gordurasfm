// Textos del Wrapped.
import '../format.dart';

class TW {
  TW._();
  static const titulo = 'Wrapped';
  static const explica = 'Tu resumen anual animado, con pruebas y tu música de fondo.';
  static const sinAnios = 'Todavía no hay escuchas. Cuando las haya, aquí aparecerá tu Wrapped.';
  static const definitivo = 'Definitivo';
  static const vistaPrevia = 'Vista previa';
  static const sinGenerar = 'Se genera al abrirlo';
  static const desactualizado = 'Hay escuchas nuevas de este año';
  static const rehacer = 'Rehacer';
  static const ver = 'Ver';
  static const generar = 'Generar';
  static String escuchasAnio(String n) => '$n escuchas';
  static const vistaPreviaExplica = 'Vista previa: el definitivo se genera el 1 de enero.';
  static const generando = 'Preparando tu año…';

  // Inicio
  static String tuWrapped(int y) => 'Tu $y en música';
  static const subeVolumen = 'Con música de tu Navidrome. Sube el volumen.';
  static const sinMusica = 'Tus canciones no están en Navidrome: esta vez irá sin música.';
  static const empezar = 'Empezar';

  // Diapositivas
  static String hola(String u) => 'Hola, $u.';
  static const intro2 = 'Esto es lo que más ha sonado en tu año.';
  static String lePisteAlPlay(int y) => 'En $y le diste al play';
  static const veces = 'veces';
  static String frenteA(String pct, bool up, int y) => 'Un $pct ${up ? 'más' : 'menos'} que en $y';
  static String mediaDia(String n) => 'Una media de $n al día';
  static const tePasaste = 'Te pasaste';
  static const minutosEscuchando = 'minutos escuchando música';
  static const aprox = '(aproximado)';
  static String diasSeguidos(String n) => 'Eso son $n días enteros sin parar';
  static String horas(String n) => 'Eso son $n horas';
  static String diasConMusica(int n, int total) => 'Sonó música $n de los $total días';
  static String horaPunta(int h) => 'Tu hora punta: las ${h.toString().padLeft(2, '0')}:00';

  static const pregunta = {
    'artist': '¿Quién ha sido tu artista del año?',
    'album': '¿Cuál ha sido tu álbum del año?',
    'track': '¿Cuál ha sido tu canción del año?',
  };
  static const elige = 'Elige una y confirma';
  static const confirmar = 'Confirmar';
  static const redoble = 'Y la respuesta es…';
  static const acierto = '¡Lo sabías!';
  static String fallo(String w) => '¡Casi! Era $w';
  static const tocaSeguir = 'Toca para seguir';

  static const ganador = {'artist': 'Tu artista del año', 'album': 'Tu álbum del año', 'track': 'Tu canción del año'};
  static String escuchas(int n) => '${fmtInt(n)} ${n == 1 ? 'escucha' : 'escuchas'}';
  static String pctDeTodo(String p) => 'El $p % de todo lo que escuchaste';
  static String laPusiste(int n) => 'La pusiste ${fmtInt(n)} veces';

  static const top5 = {'artist': 'Tus artistas', 'album': 'Tus álbumes', 'track': 'Tus canciones'};
  static const asiFue = 'Así fue tu año';
  static const trimestres = ['Ene – Mar', 'Abr – Jun', 'Jul – Sep', 'Oct – Dic'];
  static const tuMes = 'Tu mes más musical fue';
  static const loQueMas = 'Lo que más sonó ese mes';

  static String resumenTitulo(int y) => 'Mi $y en OwnFM';
  static const topArtistas = 'Top artistas';
  static const topCanciones = 'Top canciones';
  static const minutos = 'Minutos';
  static const escuchasTit = 'Escuchas';
  static String acertaste(int n, int t) => '¡Has acertado $n de $t!';
  static const compartir = 'Compartir';
  static const exportarTodo = 'Exportar todo';
  static const otraVez = 'Ver otra vez';
  static String exportando(int i, int n) => 'Exportando $i de $n…';
  static const exportado = 'Imágenes descargadas';
  static const silenciar = 'Silenciar';
  static const activarSonido = 'Activar sonido';
  static const cerrar = 'Cerrar';
}
