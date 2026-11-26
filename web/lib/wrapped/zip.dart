import 'dart:convert';
import 'dart:typed_data';

/// ZIP sin compresión (los PNG ya van comprimidos): lo justo para exportar las diapositivas.
Uint8List zipStore(List<(String, Uint8List)> files) {
  final out = BytesBuilder();
  final central = BytesBuilder();
  var offset = 0;
  void u16(BytesBuilder b, int v) => b.add([v & 0xff, (v >> 8) & 0xff]);
  void u32(BytesBuilder b, int v) => b.add([v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >> 24) & 0xff]);
  for (final (name, data) in files) {
    final n = utf8.encode(name);
    final crc = crc32(data);
    final local = BytesBuilder();
    u32(local, 0x04034b50);
    u16(local, 20); // versión necesaria
    u16(local, 0x0800); // nombres en UTF-8
    u16(local, 0); // sin compresión
    u16(local, 0); // hora
    u16(local, 0x21); // fecha (1980-01-01)
    u32(local, crc);
    u32(local, data.length);
    u32(local, data.length);
    u16(local, n.length);
    u16(local, 0);
    local.add(n);
    final header = local.takeBytes();
    out
      ..add(header)
      ..add(data);

    u32(central, 0x02014b50);
    u16(central, 20);
    u16(central, 20);
    u16(central, 0x0800);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0x21);
    u32(central, crc);
    u32(central, data.length);
    u32(central, data.length);
    u16(central, n.length);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0);
    u16(central, 0);
    u32(central, 0);
    u32(central, offset);
    central.add(n);
    offset += header.length + data.length;
  }
  final cd = central.takeBytes();
  out.add(cd);
  final end = BytesBuilder();
  u32(end, 0x06054b50);
  u16(end, 0);
  u16(end, 0);
  u16(end, files.length);
  u16(end, files.length);
  u32(end, cd.length);
  u32(end, offset);
  u16(end, 0);
  out.add(end.takeBytes());
  return out.takeBytes();
}

final _table = List<int>.generate(256, (n) {
  var c = n;
  for (var k = 0; k < 8; k++) {
    c = (c & 1) != 0 ? 0xEDB88320 ^ (c >> 1) : c >> 1;
  }
  return c;
});

int crc32(List<int> data) {
  var c = 0xFFFFFFFF;
  for (final b in data) {
    c = _table[(c ^ b) & 0xff] ^ (c >> 8);
  }
  return (c ^ 0xFFFFFFFF) & 0xFFFFFFFF;
}
