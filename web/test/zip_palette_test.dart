import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:ownfm/wrapped/model.dart';
import 'package:ownfm/wrapped/palette.dart';
import 'package:ownfm/wrapped/zip.dart';

void main() {
  test('crc32 conocido', () {
    expect(crc32(utf8.encode('123456789')), 0xCBF43926);
    expect(crc32(const []), 0);
  });

  test('zip sin compresión con la estructura correcta', () {
    final a = Uint8List.fromList(utf8.encode('hola'));
    final b = Uint8List.fromList(List.generate(300, (i) => i % 256));
    final z = zipStore([('a.txt', a), ('b.bin', b)]);
    final bd = ByteData.sublistView(z);
    expect(bd.getUint32(0, Endian.little), 0x04034b50);
    // Fin del directorio central: 22 bytes al final, con 2 entradas.
    final end = z.length - 22;
    expect(bd.getUint32(end, Endian.little), 0x06054b50);
    expect(bd.getUint16(end + 10, Endian.little), 2);
    final cdSize = bd.getUint32(end + 12, Endian.little);
    final cdOffset = bd.getUint32(end + 16, Endian.little);
    expect(cdOffset + cdSize, end);
    expect(bd.getUint32(cdOffset, Endian.little), 0x02014b50);
    // El contenido va tal cual tras la cabecera local (30 bytes + nombre).
    expect(utf8.decode(z.sublist(30 + 5, 30 + 5 + 4)), 'hola');
  });

  test('paleta: colores vivos y distintos', () {
    List<int> solid(int r, int g, int b) => [
      for (var i = 0; i < 100; i++) ...[r, g, b, 255],
    ];
    final colors = pickColors([solid(200, 30, 30), solid(30, 30, 200), solid(128, 128, 128), solid(10, 10, 10)]);
    expect(colors.length, 2, reason: 'el gris y el negro no cuentan');
    expect(pickColors([solid(128, 128, 128)]), isEmpty);
  });

  test('modelo del Wrapped', () {
    final d = WrappedData.fromJson({
      'id': 3,
      'year': 2025,
      'version': 1,
      'is_final': true,
      'outdated': false,
      'answers': {
        'artist': {'chosen_rank': 2, 'correct': false},
      },
      'data': {
        'user': 'pablo',
        'totals': {'plays': 10, 'ms': 1800000, 'artists': 3, 'albums': 3, 'tracks': 4, 'days_with_music': 5},
        'top_artists': [
          {
            'rank': 1,
            'name': 'Rosalía',
            'plays': 6,
            'image': null,
            'covers': ['img:ab', 'x'],
          },
        ],
        'top_albums': [
          {'rank': 1, 'title': 'Motomami', 'artist': 'Rosalía', 'plays': 6, 'cover': 'img:cd'},
        ],
        'top_tracks': [],
        'monthly': [
          {'month': 1, 'plays': 2, 'top_artist': 'Zahara'},
        ],
        'evolution': [],
        'top_month': {
          'month': 7,
          'plays': 5,
          'top_artist': {'name': 'Rosalía', 'plays': 5, 'image': 'img:ef', 'covers': []},
          'top_track': null,
        },
        'soundtrack': [
          {'navidrome_song_id': 's1', 'title': 'Saoko', 'artist': 'Rosalía'},
        ],
      },
    });
    expect(d.minutes, 30);
    expect(d.topArtists.first.covers, ['/api/v1/wrapped/image/ab']);
    expect(d.topAlbums.first.image, '/api/v1/wrapped/image/cd');
    expect(d.topMonthArtist?.image, '/api/v1/wrapped/image/ef');
    expect(d.topMonthTrack, isNull);
    expect(d.answers['artist']?.correct, false);
    expect(d.allImages.toSet(), {'/api/v1/wrapped/image/ab', '/api/v1/wrapped/image/cd', '/api/v1/wrapped/image/ef'});
  });
}
