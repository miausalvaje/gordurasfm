// Cliente de la API de OwnFM y modelos.

import 'dart:convert';

import 'package:http/http.dart' as http;

import 'l10n/es.dart';

class ApiException implements Exception {
  ApiException(this.status, this.code, this.message);
  final int status;
  final String code;
  final String message;

  @override
  String toString() => message;
}

class Api {
  Api({http.Client? client, this.base = ''}) : _c = client ?? http.Client();

  final http.Client _c;

  /// Prefijo de las URL (vacío en el navegador: mismo origen).
  final String base;

  /// Se llama cuando el servidor dice que no hay sesión.
  void Function()? onUnauthorized;

  Uri _uri(String path, [Map<String, String>? q]) {
    final u = Uri.parse('$base$path');
    return (q == null || q.isEmpty) ? u : u.replace(queryParameters: {...u.queryParameters, ...q});
  }

  Future<dynamic> get(String path, [Map<String, String>? q]) => _do('GET', path, q: q);

  Future<dynamic> send(String method, String path, [Object? body]) => _do(method, path, body: body);

  Future<dynamic> _do(String method, String path, {Map<String, String>? q, Object? body}) async {
    final req = http.Request(method, _uri(path, q));
    if (method != 'GET') {
      // Protección CSRF del servidor.
      req.headers['X-Requested-With'] = 'OwnFM';
      req.headers['Content-Type'] = 'application/json';
      req.body = jsonEncode(body ?? {});
    }
    late http.Response r;
    try {
      r = await http.Response.fromStream(await _c.send(req));
    } catch (_) {
      throw ApiException(0, 'network', T.errorRed);
    }
    final text = utf8.decode(r.bodyBytes);
    dynamic data;
    try {
      data = text.isEmpty ? null : jsonDecode(text);
    } catch (_) {
      data = null;
    }
    if (r.statusCode >= 200 && r.statusCode < 300) return data;
    if (r.statusCode == 401 && path != '/api/v1/auth/login') onUnauthorized?.call();
    final m = data is Map ? data : const {};
    throw ApiException(r.statusCode, (m['error'] ?? 'error').toString(), (m['message'] ?? T.errorGenerico).toString());
  }

  // ---------- Llamadas ----------

  Future<User?> me() async {
    try {
      return User.fromJson(await get('/api/v1/me'));
    } on ApiException catch (e) {
      if (e.status == 401) return null;
      rethrow;
    }
  }

  Future<User> login(String username, String password) async {
    final r = await send('POST', '/api/v1/auth/login', {'username': username, 'password': password});
    return User.fromJson(r['user']);
  }

  Future<void> logout() => send('POST', '/api/v1/auth/logout');

  Future<User> updateMe(Map<String, Object?> changes) async =>
      User.fromJson(await send('PATCH', '/api/v1/me', changes));

  Future<StatsData> stats(String period, {int limit = 10}) async =>
      StatsData.fromJson(await get('/api/v1/stats', {'period': period, 'limit': '$limit'}));

  Future<TopPage> top(String kind, String period, {int page = 1, int limit = 50}) async =>
      TopPage.fromJson(kind, await get('/api/v1/top/$kind', {'period': period, 'page': '$page', 'limit': '$limit'}));

  Future<HistoryPage> scrobbles({int? before, int limit = 50}) async => HistoryPage.fromJson(
    await get('/api/v1/scrobbles', {'limit': '$limit', if (before != null) 'before': '$before'}),
  );

  Future<NowPlaying?> nowPlaying() async {
    final r = await get('/api/v1/now-playing');
    return r == null ? null : NowPlaying.fromJson(r);
  }
}

int _i(Object? v) => v is num ? v.toInt() : int.tryParse('$v') ?? 0;
int? _in(Object? v) => v == null ? null : _i(v);
String? _s(Object? v) => v?.toString();

class User {
  User({
    required this.id,
    required this.username,
    required this.displayName,
    required this.isAdmin,
    required this.timezone,
    required this.isPublic,
    required this.createdAt,
  });

  factory User.fromJson(Map<String, dynamic> j) => User(
    id: _i(j['id']),
    username: j['username'] as String,
    displayName: (j['display_name'] ?? j['username']) as String,
    isAdmin: j['is_admin'] == true,
    timezone: (j['timezone'] ?? 'Europe/Madrid') as String,
    isPublic: j['is_public'] == true,
    createdAt: _i(j['created_at']),
  );

  final int id;
  final String username;
  final String displayName;
  final bool isAdmin;
  final String timezone;
  final bool isPublic;
  final int createdAt;
}

class ChartPoint {
  ChartPoint(this.label, this.start, this.plays);
  final String label;
  final int start;
  final int plays;
}

class Totals {
  Totals.fromJson(Map<String, dynamic> j)
    : plays = _i(j['plays']),
      listeningMs = _i(j['listening_ms']),
      withoutDuration = _i(j['plays_without_duration']),
      artists = _i(j['artists']),
      albums = _i(j['albums']),
      tracks = _i(j['tracks']),
      avgPerDay = (j['avg_plays_per_day'] as num?)?.toDouble() ?? 0,
      topDay = j['top_day'] == null ? null : (j['top_day']['date'] as String, _i(j['top_day']['plays'])),
      changePct = (j['change_pct'] as num?)?.toDouble(),
      byHour = [for (final v in (j['by_hour'] as List? ?? const [])) _i(v)],
      byWeekday = [for (final v in (j['by_weekday'] as List? ?? const [])) _i(v)];

  final int plays;
  final int listeningMs;
  final int withoutDuration;
  final int artists;
  final int albums;
  final int tracks;
  final double avgPerDay;
  final (String, int)? topDay;
  final double? changePct;
  final List<int> byHour;
  final List<int> byWeekday;
}

/// Un elemento de un top (álbum, canción o artista).
class TopItem {
  TopItem({
    required this.kind,
    required this.rank,
    required this.id,
    required this.title,
    required this.plays,
    this.artist,
    this.album,
    this.cover,
    this.covers = const [],
  });

  factory TopItem.fromJson(String kind, Map<String, dynamic> j) => TopItem(
    kind: kind,
    rank: _i(j['rank']),
    id: _i(j['id']),
    title: (j['title'] ?? j['name'] ?? '') as String,
    artist: _s(j['artist']),
    album: _s(j['album']),
    plays: _i(j['plays']),
    cover: _s(kind == 'artists' ? j['image'] : j['cover']),
    covers: [for (final c in (j['covers'] as List? ?? const [])) c as String],
  );

  final String kind;
  final int rank;
  final int id;
  final String title;
  final String? artist;
  final String? album;
  final int plays;

  /// Portada (álbumes, canciones) o imagen (artistas).
  final String? cover;

  /// Portadas para el collage de un artista sin imagen.
  final List<String> covers;
}

class StatsData {
  StatsData.fromJson(Map<String, dynamic> j)
    : period = j['period']['key'] as String,
      unit = j['period']['unit'] as String,
      from = _i(j['period']['from']),
      chart = [for (final c in j['chart'] as List) ChartPoint(c['label'] as String, _i(c['start']), _i(c['plays']))],
      totals = Totals.fromJson(j['totals']),
      topAlbums = [for (final t in j['top_albums'] as List) TopItem.fromJson('albums', t)],
      topTracks = [for (final t in j['top_tracks'] as List) TopItem.fromJson('tracks', t)],
      topArtists = [for (final t in j['top_artists'] as List) TopItem.fromJson('artists', t)];

  final String period;
  final String unit;
  final int from;
  final List<ChartPoint> chart;
  final Totals totals;
  final List<TopItem> topAlbums;
  final List<TopItem> topTracks;
  final List<TopItem> topArtists;
}

class TopPage {
  TopPage.fromJson(String kind, Map<String, dynamic> j)
    : total = _i(j['total']),
      page = _i(j['page']),
      items = [for (final t in j['items'] as List) TopItem.fromJson(kind, t)];

  final int total;
  final int page;
  final List<TopItem> items;
}

class Scrobble {
  Scrobble.fromJson(Map<String, dynamic> j)
    : id = _i(j['id']),
      playedAt = _i(j['played_at']),
      artist = j['artist'] as String,
      track = j['track'] as String,
      album = _s(j['album']),
      cover = _s(j['cover']),
      source = (j['source'] ?? '') as String;

  final int id;
  final int playedAt;
  final String artist;
  final String track;
  final String? album;
  final String? cover;
  final String source;
}

class HistoryPage {
  HistoryPage.fromJson(Map<String, dynamic> j)
    : total = _i(j['total']),
      items = [for (final s in j['items'] as List) Scrobble.fromJson(s)],
      nextBefore = _in(j['next_before']);

  final int total;
  final List<Scrobble> items;
  final int? nextBefore;
}

class NowPlaying {
  NowPlaying.fromJson(Map<String, dynamic> j)
    : artist = j['artist'] as String,
      track = j['track'] as String,
      album = _s(j['album']);

  final String artist;
  final String track;
  final String? album;
}
