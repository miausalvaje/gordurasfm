// Modelo del Wrapped (la «foto fija» que guarda el servidor).

/// URL de una imagen guardada (`img:<hash>`) o null.
String? imgUrl(dynamic ref) {
  if (ref is! String || !ref.startsWith('img:')) return null;
  return '/api/v1/wrapped/image/${ref.substring(4)}';
}

class WItem {
  WItem({
    required this.rank,
    required this.title,
    this.subtitle,
    required this.plays,
    this.image,
    this.covers = const [],
    this.round = false,
  });

  final int rank;
  final String title;
  final String? subtitle;
  final int plays;
  final String? image;

  /// Portadas para un collage (artistas sin imagen).
  final List<String> covers;

  /// Imagen redonda (artistas).
  final bool round;

  static WItem artist(Map<String, dynamic> j) => WItem(
    rank: (j['rank'] ?? 0) as int,
    title: (j['name'] ?? '') as String,
    plays: (j['plays'] ?? 0) as int,
    image: imgUrl(j['image']),
    covers: [for (final c in (j['covers'] ?? const []) as List) ?imgUrl(c)],
    round: true,
  );

  static WItem album(Map<String, dynamic> j) => WItem(
    rank: (j['rank'] ?? 0) as int,
    title: (j['title'] ?? '') as String,
    subtitle: j['artist'] as String?,
    plays: (j['plays'] ?? 0) as int,
    image: imgUrl(j['cover']),
  );

  static WItem track(Map<String, dynamic> j) => WItem(
    rank: (j['rank'] ?? 0) as int,
    title: (j['title'] ?? '') as String,
    subtitle: j['artist'] as String?,
    plays: (j['plays'] ?? 0) as int,
    image: imgUrl(j['cover']),
  );
}

class WMonth {
  WMonth(this.month, this.plays, this.topArtist);
  final int month;
  final int plays;
  final String? topArtist;
}

class WQuarter {
  WQuarter(this.quarter, this.plays, this.artists, this.image, this.covers);
  final int quarter;
  final int plays;
  final List<String> artists;
  final String? image;
  final List<String> covers;
}

class WSong {
  WSong(this.id, this.title, this.artist);
  final String id;
  final String title;
  final String artist;
}

class WrappedData {
  WrappedData({
    required this.id,
    required this.year,
    required this.version,
    required this.isFinal,
    required this.outdated,
    required this.user,
    required this.plays,
    required this.ms,
    required this.msEstimated,
    required this.artists,
    required this.albums,
    required this.tracks,
    required this.daysWithMusic,
    required this.peakHour,
    required this.firstPlay,
    required this.previousPlays,
    required this.changePct,
    required this.topArtists,
    required this.topAlbums,
    required this.topTracks,
    required this.monthly,
    required this.evolution,
    required this.topMonth,
    required this.topMonthPlays,
    required this.topMonthArtist,
    required this.topMonthTrack,
    required this.soundtrack,
    required this.answers,
  });

  final int id;
  final int year;
  final int version;
  final bool isFinal;
  final bool outdated;
  final String user;
  final int plays;
  final int ms;
  final bool msEstimated;
  final int artists;
  final int albums;
  final int tracks;
  final int daysWithMusic;
  final int? peakHour;

  /// Primera escucha del año (UTC, s).
  final int? firstPlay;
  final int? previousPlays;
  final double? changePct;
  final List<WItem> topArtists;
  final List<WItem> topAlbums;
  final List<WItem> topTracks;
  final List<WMonth> monthly;
  final List<WQuarter> evolution;
  final int? topMonth;
  final int topMonthPlays;
  final WItem? topMonthArtist;
  final WItem? topMonthTrack;
  final List<WSong> soundtrack;

  /// Respuestas guardadas: pregunta → (posición elegida, acierto).
  final Map<String, ({int chosenRank, bool correct})> answers;

  int get minutes => ms ~/ 60000;

  /// Todas las imágenes, para cargarlas antes de empezar.
  Iterable<String> get allImages sync* {
    for (final l in [topArtists, topAlbums, topTracks]) {
      for (final i in l) {
        if (i.image != null) yield i.image!;
        yield* i.covers;
      }
    }
    for (final q in evolution) {
      if (q.image != null) yield q.image!;
      yield* q.covers;
    }
    for (final i in [topMonthArtist, topMonthTrack]) {
      if (i?.image != null) yield i!.image!;
      if (i != null) yield* i.covers;
    }
  }

  factory WrappedData.fromJson(Map<String, dynamic> j) {
    final d = j['data'] as Map<String, dynamic>;
    final t = d['totals'] as Map<String, dynamic>;
    final tm = d['top_month'] as Map<String, dynamic>?;
    final tma = tm?['top_artist'] as Map<String, dynamic>?;
    final tmt = tm?['top_track'] as Map<String, dynamic>?;
    final ans = <String, ({int chosenRank, bool correct})>{};
    ((j['answers'] ?? const {}) as Map).forEach((k, v) {
      ans[k as String] = (chosenRank: (v['chosen_rank'] ?? 0) as int, correct: v['correct'] == true);
    });
    List<Map<String, dynamic>> list(String k) => ((d[k] ?? const []) as List).cast<Map<String, dynamic>>();
    return WrappedData(
      id: j['id'] as int,
      year: j['year'] as int,
      version: (j['version'] ?? 0) as int,
      isFinal: j['is_final'] == true,
      outdated: j['outdated'] == true,
      user: (d['user'] ?? '') as String,
      plays: (t['plays'] ?? 0) as int,
      ms: (t['ms'] ?? 0) as int,
      msEstimated: t['ms_estimated'] == true,
      artists: (t['artists'] ?? 0) as int,
      albums: (t['albums'] ?? 0) as int,
      tracks: (t['tracks'] ?? 0) as int,
      daysWithMusic: (t['days_with_music'] ?? 0) as int,
      peakHour: t['peak_hour'] as int?,
      firstPlay: t['first_play'] as int?,
      previousPlays: t['previous_plays'] as int?,
      changePct: (t['change_pct'] as num?)?.toDouble(),
      topArtists: list('top_artists').map(WItem.artist).toList(),
      topAlbums: list('top_albums').map(WItem.album).toList(),
      topTracks: list('top_tracks').map(WItem.track).toList(),
      monthly: [
        for (final m in list('monthly'))
          WMonth((m['month'] ?? 0) as int, (m['plays'] ?? 0) as int, m['top_artist'] as String?),
      ],
      evolution: [
        for (final q in list('evolution'))
          WQuarter(
            (q['quarter'] ?? 0) as int,
            (q['plays'] ?? 0) as int,
            [for (final a in (q['top_artists'] ?? const []) as List) (a['name'] ?? '') as String],
            imgUrl(q['image']),
            [for (final c in (q['covers'] ?? const []) as List) ?imgUrl(c)],
          ),
      ],
      topMonth: tm?['month'] as int?,
      topMonthPlays: (tm?['plays'] ?? 0) as int,
      topMonthArtist: tma == null ? null : WItem.artist({...tma, 'rank': 1}),
      topMonthTrack: tmt == null ? null : WItem.track({...tmt, 'rank': 1}),
      soundtrack: [
        for (final s in list('soundtrack'))
          WSong(s['navidrome_song_id'] as String, (s['title'] ?? '') as String, (s['artist'] ?? '') as String),
      ],
      answers: ans,
    );
  }
}
