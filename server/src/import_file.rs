//! Lectura de ficheros de historial: CSV (exportaciones de Last.fm, propio de GordurasFM)
//! y JSON (ListenBrainz y propio, en lista o una escucha por línea).

use chrono::{NaiveDateTime, TimeZone, Utc};
use serde_json::Value;

use crate::scrobble::{Listen, Source};

/// Separa una línea CSV respetando comillas (`"a, b"` y `""` escapadas).
pub fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if cur.is_empty() => quoted = true,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/// Registros de un CSV, uno a uno (una comilla abierta puede abarcar varias líneas).
struct CsvRecords<'a> {
    lines: std::str::Lines<'a>,
}

impl Iterator for CsvRecords<'_> {
    type Item = String;

    fn next(&mut self) -> Option<String> {
        let mut cur = String::new();
        for line in self.lines.by_ref() {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
            if cur.matches('"').count().is_multiple_of(2) {
                if cur.trim().is_empty() {
                    cur.clear();
                    continue;
                }
                return Some(cur);
            }
        }
        (!cur.trim().is_empty()).then_some(cur)
    }
}

/// Fecha en UTC: segundos/milisegundos Unix, RFC 3339 o el formato de Last.fm («31 Jan 2021 14:03»).
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(n) = s.parse::<i64>() {
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.timestamp());
    }
    for f in ["%d %b %Y %H:%M", "%d %b %Y, %H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(s, f) {
            return Some(Utc.from_utc_datetime(&n).timestamp());
        }
    }
    None
}

fn listen(
    artist: &str,
    track: &str,
    album: Option<&str>,
    at: i64,
    duration_ms: Option<i64>,
    raw: Option<Value>,
    source: Source,
) -> Option<Listen> {
    let (artist, track) = (artist.trim(), track.trim());
    if artist.is_empty() || track.is_empty() {
        return None;
    }
    Some(Listen {
        artist: artist.to_string(),
        track: track.to_string(),
        album: album.map(str::trim).filter(|a| !a.is_empty()).map(str::to_string),
        album_artist: None,
        duration_ms,
        played_at: at,
        recording_mbid: None,
        release_mbid: None,
        artist_mbid: None,
        client: Some("importación".into()),
        source,
        raw,
        cover_url: None,
    })
}

/// Cada registro del fichero: `Some(escucha)` si se ha podido leer, `None` si no.
pub type Records<'a> = Box<dyn Iterator<Item = Option<Listen>> + Send + 'a>;

/// Recorre un fichero de historial sin cargar todas las escuchas en memoria.
/// Devuelve el formato detectado y el iterador de registros.
pub fn records(text: &str) -> Result<(&'static str, Records<'_>), String> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.trim_start().chars().next().ok_or("el fichero está vacío")?;
    match first {
        '[' => {
            // Se guardan solo trozos del texto original (sin copiar) y cada escucha se lee al pedirla.
            let raws: Vec<&serde_json::value::RawValue> =
                serde_json::from_str(text).map_err(|e| format!("JSON no válido: {e}"))?;
            Ok((
                "json",
                Box::new(
                    raws.into_iter().map(|r| serde_json::from_str::<Value>(r.get()).ok().and_then(|v| json_item(&v))),
                ),
            ))
        }
        '{' => {
            // Un objeto con `payload` (petición de ListenBrainz) o una escucha por línea.
            let one_line = text.trim().lines().count() == 1;
            if one_line
                && let Ok(v) = serde_json::from_str::<Value>(text)
                && let Some(Value::Array(p)) = v.get("payload")
            {
                let items: Vec<Option<Listen>> = p.iter().map(json_item).collect();
                return Ok(("json", Box::new(items.into_iter())));
            }
            Ok((
                "jsonl",
                Box::new(
                    text.lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| serde_json::from_str::<Value>(l).ok().and_then(|v| json_item(&v))),
                ),
            ))
        }
        _ => Ok(("csv", csv_iter(text))),
    }
}

/// Cuenta escuchas válidas y no válidas (para avisar al subir el fichero).
pub fn summarize(text: &str) -> Result<(&'static str, usize, usize), String> {
    let (format, it) = records(text)?;
    let (mut ok, mut bad) = (0, 0);
    for r in it {
        if r.is_some() { ok += 1 } else { bad += 1 }
    }
    Ok((format, ok, bad))
}

#[derive(Debug, Default)]
pub struct Parsed {
    pub listens: Vec<Listen>,
    /// Registros que no se pudieron leer (sin artista, título o fecha válida).
    pub invalid: usize,
    pub format: &'static str,
}

/// Lee el fichero entero (para ficheros pequeños y tests).
pub fn parse(text: &str) -> Result<Parsed, String> {
    let (format, it) = records(text)?;
    let mut p = Parsed { format, ..Default::default() };
    for r in it {
        match r {
            Some(l) => p.listens.push(l),
            None => p.invalid += 1,
        }
    }
    Ok(p)
}

fn json_item(v: &Value) -> Option<Listen> {
    if let Some(md) = v.get("track_metadata") {
        // ListenBrainz: se guarda el original (trae MBIDs y más datos).
        let info = md.get("additional_info");
        let dur = info.and_then(|i| i.get("duration_ms")).and_then(Value::as_i64);
        let a = md.get("artist_name").and_then(Value::as_str)?;
        let t = md.get("track_name").and_then(Value::as_str)?;
        let ts = v.get("listened_at")?;
        let ts = ts.as_i64().or_else(|| ts.as_str().and_then(parse_date))?;
        listen(a, t, md.get("release_name").and_then(Value::as_str), ts, dur, Some(v.clone()), Source::ImportFile)
    } else {
        // GordurasFM (exportación propia) u otros con campos sencillos: los campos ya son el original.
        let s = |k: &[&str]| k.iter().find_map(|k| v.get(*k).and_then(Value::as_str));
        let ts = ["played_at", "uts", "timestamp", "date"].iter().find_map(|k| match v.get(*k) {
            Some(Value::Number(n)) => n.as_i64(),
            Some(Value::String(s)) => parse_date(s),
            _ => None,
        })?;
        let source = if s(&["source"]) == Some("lastfm") { Source::Lastfm } else { Source::ImportFile };
        listen(
            s(&["artist", "artist_name"])?,
            s(&["track", "track_name", "title", "name"])?,
            s(&["album", "release_name"]),
            ts,
            v.get("duration_ms").and_then(Value::as_i64),
            None,
            source,
        )
    }
}

fn csv_iter(text: &str) -> Records<'_> {
    let mut recs = CsvRecords { lines: text.lines() }.peekable();
    let header: Vec<String> =
        recs.peek().map(|h| split_csv_line(h).iter().map(|h| h.trim().to_lowercase()).collect()).unwrap_or_default();
    let has_header = header.iter().any(|h| h == "artist" || h == "artist_name")
        && header.iter().any(|h| matches!(h.as_str(), "track" | "name" | "title" | "track_name"));
    let col = |names: &[&str]| header.iter().position(|h| names.contains(&h.as_str()));
    let (ia, ial, it, id, idur, isrc) = if has_header {
        recs.next();
        (
            col(&["artist", "artist_name"]),
            col(&["album", "release_name"]),
            col(&["track", "name", "title", "track_name"]),
            col(&["uts", "played_at", "timestamp", "date", "utc_time", "time"]),
            col(&["duration_ms"]),
            col(&["source"]),
        )
    } else {
        // Formato clásico de lastfm-to-csv: artist,album,track,date
        (Some(0), Some(1), Some(2), Some(3), None, None)
    };
    Box::new(recs.map(move |rec| {
        let f = split_csv_line(&rec);
        let get = |i: Option<usize>| i.and_then(|i| f.get(i)).map(String::as_str);
        let ts = get(id).and_then(parse_date)?;
        let source = if get(isrc) == Some("lastfm") || !has_header { Source::Lastfm } else { Source::ImportFile };
        let dur = get(idur).and_then(|d| d.trim().parse::<i64>().ok());
        listen(get(ia)?, get(it)?, get(ial), ts, dur, None, source)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_clasico_de_lastfm_to_csv() {
        let txt = "Rosalía,Motomami,Saoko,14 Nov 2023 22:13\n\"Love, Death & Robots\",\"Álbum \"\"raro\"\"\",Tema,01 Jan 2020 00:00\nmal,,,\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.format, "csv");
        assert_eq!(p.listens.len(), 2);
        assert_eq!(p.invalid, 1);
        assert_eq!(p.listens[0].played_at, 1_700_000_000 - 20);
        assert_eq!(p.listens[1].artist, "Love, Death & Robots");
        assert_eq!(p.listens[1].album.as_deref(), Some("Álbum \"raro\""));
        assert_eq!(p.listens[0].source, Source::Lastfm);
    }

    #[test]
    fn csv_con_cabecera() {
        let txt = "uts,utc_time,artist,artist_mbid,album,album_mbid,track,track_mbid\n1700000000,\"14 Nov 2023, 22:13\",Rosalía,,Motomami,,Saoko,\n";
        let p = parse(txt).unwrap();
        assert_eq!(p.listens.len(), 1);
        assert_eq!(p.listens[0].played_at, 1_700_000_000);
        assert_eq!(p.listens[0].track, "Saoko");
    }

    #[test]
    fn json_de_listenbrainz_y_propio() {
        let lb = r#"{"listened_at": 1700000000, "track_metadata": {"artist_name": "A", "track_name": "B", "release_name": "C"}}
{"listened_at": 1700000300, "track_metadata": {"artist_name": "A", "track_name": "D"}}"#;
        let p = parse(lb).unwrap();
        assert_eq!((p.format, p.listens.len()), ("jsonl", 2));
        let own = r#"[{"played_at": 1700000000, "artist": "A", "track": "B", "album": null, "duration_ms": 200000, "source": "lastfm"},
                      {"artist": "sin fecha", "track": "x"}]"#;
        let p = parse(own).unwrap();
        assert_eq!((p.listens.len(), p.invalid), (1, 1));
        assert_eq!(p.listens[0].duration_ms, Some(200000));
        assert_eq!(p.listens[0].source, Source::Lastfm);
    }

    #[test]
    fn fechas() {
        assert_eq!(parse_date("1700000000"), Some(1_700_000_000));
        assert_eq!(parse_date("1700000000000"), Some(1_700_000_000));
        assert_eq!(parse_date("2023-11-14T22:13:20Z"), Some(1_700_000_000));
        assert_eq!(parse_date("ayer"), None);
    }
}
