// Todos los textos de la interfaz, en español. Para traducir, copiar este archivo.
// ignore_for_file: constant_identifier_names

class T {
  T._();

  static const app = 'GordurasFM';

  // Navegación
  static const navInicio = 'Inicio';
  static const navTops = 'Tops';
  static const navHistorial = 'Historial';
  static const navPerfil = 'Perfil';
  static const navAjustes = 'Conexiones y ajustes';
  static const navAdmin = 'Administración';

  // Login
  static const loginTitulo = 'Tus escuchas, tus números';
  static const loginExplica = 'Entra con tu usuario y contraseña de Navidrome.';
  static const loginUsuario = 'Usuario';
  static const loginContrasena = 'Contraseña';
  static const loginEntrar = 'Entrar';
  static const loginMal = 'Usuario o contraseña incorrectos.';
  static const loginNota = 'La contraseña no se guarda: solo se usa para comprobarla con Navidrome.';
  static const salir = 'Cerrar sesión';

  // Periodos
  static const periodos = <String, String>{
    'this_week': 'Esta semana',
    'this_month': 'Este mes',
    'this_year': 'Este año',
    'last_week': 'Semana pasada',
    'last_month': 'Mes pasado',
    'last_year': 'Año pasado',
    'all_time': 'Siempre',
  };

  // Estadísticas
  static const escuchas = 'escuchas';
  static const escuchasMayus = 'Escuchas';
  static const tiempoEscuchado = 'Tiempo escuchado';
  static const artistas = 'Artistas';
  static const albumes = 'Álbumes';
  static const canciones = 'Canciones';
  static const mediaDiaria = 'Media diaria';
  static const diaTop = 'Mejor día';
  static const frenteAnterior = 'frente al periodo anterior';
  static const sinDuracion = 'Sin contar escuchas de duración desconocida';
  static String porUnidad(String unit) => switch (unit) {
    'day' => 'Escuchas por día',
    'month' => 'Escuchas por mes',
    _ => 'Escuchas por año',
  };
  static const cuandoEscuchas = 'A qué hora escuchas';
  static const topAlbums = 'Top álbumes';
  static const topTracks = 'Top canciones';
  static const topArtists = 'Top artistas';
  static const verTodos = 'Ver todos';
  static const verHistorial = 'Ver historial';
  static const vacioTitulo = 'Todavía no hay escuchas aquí';
  static const vacioTexto = 'Conecta Navidrome para que tus escuchas lleguen solas, o importa tu historial de Last.fm.';
  static const irConexiones = 'Ir a conexiones';
  static const escuchandoAhora = 'Escuchando ahora';
  static String hola(String n) => 'Hola, $n';

  // Tops
  static const tipoTop = <String, String>{'albums': 'Álbumes', 'tracks': 'Canciones', 'artists': 'Artistas'};
  static String total(int n, String kind) => switch (kind) {
    'albums' => '$n álbumes',
    'tracks' => '$n canciones',
    _ => '$n artistas',
  };

  // Historial
  static String historialTotal(String n) => '$n escuchas en total';
  static const hoy = 'Hoy';
  static const ayer = 'Ayer';
  static const fuente = <String, String>{
    'navidrome': 'Navidrome',
    'navidrome_poll': 'Navidrome',
    'lastfm': 'Last.fm',
    'import_file': 'Importada',
  };

  // Perfil
  static String miembroDesde(String f) => 'En GordurasFM desde $f';
  static const enTotal = 'En total';
  static const recientes = 'Escuchado hace poco';
  static const wrapped = 'Wrapped';
  static const navWrapped = 'Wrapped';
  static const editarPerfil = 'Editar perfil';
  static const nombreVisible = 'Nombre visible';
  static const zonaHoraria = 'Zona horaria';
  static String usarZonaNavegador(String z) => 'Usar la del navegador ($z)';
  static const zonaExplica = 'Las semanas, los meses y los años se cuentan con esta zona horaria.';
  static const guardar = 'Guardar';
  static const cancelar = 'Cancelar';
  static const guardado = 'Guardado';

  // Conexiones
  static const conexiones = 'Conexiones';
  static const navidromeTitulo = 'Navidrome → GordurasFM';
  static const navidromeExplica =
      'Para que tus escuchas lleguen solas, pega un token de escucha en Navidrome: Ajustes personales → ListenBrainz.';
  static const navidromeAdmin =
      'El administrador de Navidrome tiene que configurar esto una sola vez, para todo el servidor:';
  static const generarToken = 'Generar token de escucha';
  static const tokenNuevo = 'Copia este token ahora: no se volverá a mostrar.';
  static const copiar = 'Copiar';
  static const copiado = 'Copiado';
  static const borrar = 'Borrar';
  static String tokenHint(String h) => 'Termina en …$h';
  static String ultimoUso(String s) => 'Último uso: $s';
  static const nuncaUsado = 'Sin usar todavía';
  static const listenbrainz = 'ListenBrainz';
  static const lbExplica =
      'Pega tu token de ListenBrainz (listenbrainz.org → Ajustes) para que tus escuchas sigan llegando allí. '
      'Si ya lo tienes puesto en Navidrome, no hace falta cambiarlo.';
  static const lbToken = 'Token de ListenBrainz';
  static const conectar = 'Conectar';
  static const desconectar = 'Desconectar';
  static const reenviar = 'Reenviar mis escuchas';
  static String conectadoComo(String u) => 'Conectado como $u';
  static const reconectar = 'La conexión ha fallado: vuelve a conectar.';
  static const lastfm = 'Last.fm';
  static const lfmExplica = 'Conecta tu cuenta para importar tu historial y, si quieres, enviar allí tus escuchas.';
  static const lfmSinKey = 'Falta la API key de Last.fm. Pídesela al administrador o pon la tuya.';
  static const lfmConectar = 'Conectar con Last.fm';
  static const lfmAvisoDuplicado =
      'Si Navidrome ya envía tus escuchas a Last.fm, deja esto apagado o saldrán duplicadas.';
  static const lfmPropiaKey = 'Usar mi propia API key';
  static const lfmKey = 'API key';
  static const lfmSecret = 'Secret';
  static const lfmOk = 'Last.fm conectado';
  static const lfmError = 'No se pudo conectar con Last.fm';
  static const cola = 'Reenvíos';
  static const estadosCola = <String, String>{
    'pending': 'pendientes',
    'done': 'enviadas',
    'paused': 'en pausa',
    'failed': 'fallidas',
    'dead': 'descartadas',
  };
  static const reintentar = 'Reintentar';

  // Importar y exportar
  static const importar = 'Importar historial';
  static const importarLfmTodo = 'Importar todo de Last.fm';
  static const importarLfmNuevo = 'Traer solo lo nuevo';
  static const usuarioLfm = 'Usuario de Last.fm';
  static const importarFichero = 'Subir fichero (CSV o JSON)';
  static const importarFicheroExplica =
      'Exportaciones de Last.fm (lastfm-to-csv), de ListenBrainz o de GordurasFM. Las repetidas no se duplican.';
  static const exportar = 'Exportar mi historial';
  static const trabajos = <String, String>{
    'lastfm_import': 'Importación de Last.fm',
    'file_import': 'Importación de fichero',
  };
  static const estadosTrabajo = <String, String>{
    'queued': 'En cola',
    'running': 'En marcha',
    'done': 'Terminada',
    'failed': 'Fallida',
    'cancelled': 'Cancelada',
  };
  static String resumenImport(int g, int d, int i) => '$g nuevas · $d repetidas · $i no válidas';
  static const reanudar = 'Reanudar';
  static const cancelarTrabajo = 'Cancelar';

  // Admin
  static const adminLfm = 'API key de Last.fm de la instancia';
  static const adminLfmExplica =
      'Se crea gratis en last.fm/api/account/create. La usan todos los que no pongan la suya.';
  static const adminLfmConfig = 'Configurada en el archivo de configuración del servidor.';
  static const usuarios = 'Usuarios';
  static const aprobado = 'Aprobado';
  static const admin = 'Admin';

  // Errores
  static const errorRed = 'No se pudo contactar con el servidor.';
  static const errorGenerico = 'Algo ha fallado.';
  static const reintentarCarga = 'Reintentar';
}
