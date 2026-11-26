//! Tests de integración de la fase 4: Wrapped (foto fija, versiones, pruebas, imágenes propias),
//! audio de Navidrome y generación automática del definitivo.

#![allow(clippy::field_reassign_with_default)]

use std::collections::HashMap;

use chrono::{Datelike, TimeZone, Utc};
use gordurasfm::{
    AppState,
    config::Config,
    crypto::Crypto,
    db,
    scrobble::{self, Listen, Source},
    state,
};
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000IHDRfalso";
const MP3: &[u8] = b"ID3\x03\x00\x00\x00\x00\x00\x00audio-falso";

struct TestApp {
    base: String,
    http: reqwest::Client,
    st: AppState,
}

fn subsonic(req: &Request) -> ResponseTemplate {
    let q: HashMap<String, String> = req.url.query_pairs().into_owned().collect();
    let pass = match q.get("u").map(String::as_str) {
        Some("ana") => "secreto",
        Some("bea") => "clave",
        _ => "",
    };
    let ok = !pass.is_empty()
        && q.get("t") == Some(&gordurasfm::crypto::subsonic_token(pass, q.get("s").map(String::as_str).unwrap_or("")));
    let wrap = |extra: Value| {
        let mut r = json!({"status": "ok", "version": "1.16.1"});
        if let (Some(o), Some(e)) = (r.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        ResponseTemplate::new(200).set_body_json(json!({"subsonic-response": r}))
    };
    if !ok {
        return ResponseTemplate::new(200).set_body_json(
            json!({"subsonic-response": {"status": "failed", "error": {"code": 40, "message": "Wrong username or password"}}}),
        );
    }
    let p = req.url.path();
    if p.ends_with("/getUser") {
        wrap(json!({"user": {"username": q["u"], "adminRole": q["u"] == "ana"}}))
    } else if p.ends_with("/search3") {
        wrap(json!({"searchResult3": {
            "artist": [{"id": "ar1", "name": "Rosalía", "coverArt": "ar-ar1"}],
            "album": [{"id": "al1", "name": "Motomami", "artist": "Rosalía", "coverArt": "al-al1"}],
            "song": [{"id": "s1", "title": "Saoko", "artist": "Rosalía", "album": "Motomami", "albumId": "al1", "coverArt": "al-al1", "duration": 137}]
        }}))
    } else if p.ends_with("/getCoverArt") {
        ResponseTemplate::new(200).insert_header("content-type", "image/png").set_body_bytes(PNG)
    } else if p.ends_with("/stream") {
        assert_eq!(q.get("id").map(String::as_str), Some("s1"));
        let range = req.headers.get("range").and_then(|v| v.to_str().ok()).unwrap_or("");
        if range == "bytes=3-" {
            ResponseTemplate::new(206)
                .insert_header("content-type", "audio/mpeg")
                .insert_header("content-range", format!("bytes 3-{}/{}", MP3.len() - 1, MP3.len()))
                .insert_header("accept-ranges", "bytes")
                .set_body_bytes(&MP3[3..])
        } else {
            ResponseTemplate::new(200).insert_header("content-type", "audio/mpeg").set_body_bytes(MP3)
        }
    } else {
        wrap(json!({}))
    }
}

async fn spawn() -> TestApp {
    let nd = MockServer::start().await;
    Mock::given(method("GET")).respond_with(subsonic).mount(&nd).await;
    let data = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = data.keep();
    cfg.navidrome.url = nd.uri();
    cfg.listenbrainz.api_url = "http://127.0.0.1:9".into();
    cfg.enrich.enabled = false;
    cfg.validate().unwrap();
    let pool = db::open_memory().await.unwrap();
    let st = state::new(cfg, pool, Crypto::from_key(&[7u8; 32]));
    let app = gordurasfm::app(st.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    // Se deja vivo el servidor simulado de Navidrome durante todo el test.
    std::mem::forget(nd);
    TestApp { base: format!("http://{addr}"), http: gordurasfm::http_client::build(), st }
}

struct S {
    cookie: String,
    id: i64,
}

impl TestApp {
    async fn login(&self, u: &str, p: &str) -> S {
        let r = self
            .http
            .post(format!("{}/api/v1/auth/login", self.base))
            .header("X-Requested-With", "t")
            .json(&json!({"username": u, "password": p}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let cookie = r.headers()["set-cookie"].to_str().unwrap().split(';').next().unwrap().to_string();
        let v: Value = r.json().await.unwrap();
        S { cookie, id: v["user"]["id"].as_i64().unwrap() }
    }

    async fn get(&self, s: &S, p: &str) -> (u16, Value) {
        let r = self.http.get(format!("{}{p}", self.base)).header("Cookie", &s.cookie).send().await.unwrap();
        let code = r.status().as_u16();
        (code, r.json().await.unwrap_or(Value::Null))
    }

    async fn post(&self, s: &S, p: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .post(format!("{}{p}", self.base))
            .header("Cookie", &s.cookie)
            .header("X-Requested-With", "t")
            .json(&body)
            .send()
            .await
            .unwrap();
        let code = r.status().as_u16();
        (code, r.json().await.unwrap_or(Value::Null))
    }

    async fn add(&self, user: i64, artist: &str, track: &str, album: &str, at: i64) {
        let l = Listen {
            artist: artist.into(),
            track: track.into(),
            album: Some(album.into()),
            album_artist: None,
            duration_ms: Some(180_000),
            played_at: at,
            recording_mbid: None,
            release_mbid: None,
            artist_mbid: None,
            client: None,
            source: Source::Navidrome,
            raw: None,
            cover_url: None,
        };
        scrobble::ingest(&self.st.db, user, &l).await.unwrap();
    }
}

fn last_year() -> i32 {
    Utc::now().year() - 1
}

/// Mediodía (hora de Madrid) de un día del año pasado; `n` separa escuchas del mismo día.
fn ly(month: u32, day: u32, n: i64) -> i64 {
    chrono_tz::Europe::Madrid.with_ymd_and_hms(last_year(), month, day, 12, 0, 0).unwrap().timestamp() + n * 300
}

/// Año pasado: Rosalía 6 (Saoko 4, Candy 2), Bad Gyal 3, Zahara 1. Julio es el mes con más escuchas.
async fn seed(app: &TestApp, user: i64) {
    for n in 0..4 {
        app.add(user, "Rosalía", "Saoko", "Motomami", ly(7, 1, n)).await;
    }
    app.add(user, "Rosalía", "Candy", "Motomami", ly(3, 2, 0)).await;
    app.add(user, "Rosalía", "Candy", "Motomami", ly(7, 2, 0)).await;
    for n in 0..3 {
        app.add(user, "Bad Gyal", "Fiebre", "Slow Wine", ly(11, 20, n)).await;
    }
    app.add(user, "Zahara", "Berlín U5", "Puta", ly(1, 15, 0)).await;
}

#[tokio::test]
async fn wrapped_foto_fija_versiones_y_pruebas() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let bea = app.login("bea", "clave").await;
    seed(&app, ana.id).await;
    app.add(ana.id, "Rosalía", "Saoko", "Motomami", Utc::now().timestamp() - 3600).await;
    gordurasfm::enrich::pass(&app.st).await.unwrap();
    let y = last_year();

    let (code, list) = app.get(&ana, "/api/v1/wrapped").await;
    assert_eq!(code, 200, "{list}");
    let years = list["years"].as_array().unwrap();
    assert_eq!(years.len(), 2);
    assert_eq!(years[0]["current"], true);
    assert_eq!(years[1]["year"], y);
    assert_eq!(years[1]["plays"], 10);
    assert!(years[1]["wrapped"].is_null(), "todavía no se ha generado");

    let (code, w) = app.get(&ana, &format!("/api/v1/wrapped/{y}")).await;
    assert_eq!(code, 200, "{w}");
    assert_eq!((w["is_final"].as_bool(), w["version"].as_i64()), (Some(true), Some(1)));
    let d = &w["data"];
    assert_eq!(d["schema"], 1);
    assert_eq!(d["totals"]["plays"], 10);
    assert_eq!(d["totals"]["ms"], 10 * 180_000);
    assert_eq!(d["totals"]["artists"], 3);
    assert_eq!(d["totals"]["days_with_music"], 5);
    assert_eq!(d["totals"]["peak_hour"], 12);
    assert_eq!(d["totals"]["first_play"], ly(1, 15, 0), "primera escucha del año");
    assert_eq!(d["top_artists"][0]["name"], "Rosalía");
    assert_eq!(d["top_artists"][0]["plays"], 6);
    assert_eq!(d["top_artists"].as_array().unwrap().len(), 3, "solo los que hay");
    assert_eq!(d["top_tracks"][0]["title"], "Saoko");
    assert_eq!(d["top_albums"][0]["title"], "Motomami");
    assert_eq!(d["top_month"]["month"], 7);
    assert_eq!(d["top_month"]["plays"], 5);
    assert_eq!(d["top_month"]["top_artist"]["name"], "Rosalía");
    assert_eq!(d["monthly"].as_array().unwrap().len(), 12);
    assert_eq!(d["monthly"][10]["top_artist"], "Bad Gyal");
    assert_eq!(d["evolution"][3]["top_artists"][0]["name"], "Bad Gyal");
    assert_eq!(d["soundtrack"][0]["navidrome_song_id"], "s1");

    // Imágenes: copia propia, servida solo a quien tiene ese Wrapped.
    let image = d["top_artists"][0]["image"].as_str().expect("imagen del artista").to_string();
    let hash = image.strip_prefix("img:").unwrap();
    assert_eq!(d["top_albums"][0]["cover"].as_str().unwrap().len(), 4 + 64);
    let url = format!("{}/api/v1/wrapped/image/{hash}", app.base);
    let r = app.http.get(&url).header("Cookie", &ana.cookie).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(&r.bytes().await.unwrap()[..], PNG);
    assert_eq!(app.http.get(&url).header("Cookie", &bea.cookie).send().await.unwrap().status(), 404);

    // Pruebas: cuenta la primera respuesta.
    let id = w["id"].as_i64().unwrap();
    let (c, a) =
        app.post(&ana, &format!("/api/v1/wrapped/answers/{id}"), json!({"question": "artist", "chosen_rank": 2})).await;
    assert_eq!(c, 200, "{a}");
    assert_eq!(a["correct"], false);
    let (_, a) =
        app.post(&ana, &format!("/api/v1/wrapped/answers/{id}"), json!({"question": "artist", "chosen_rank": 1})).await;
    assert_eq!(a["answers"]["artist"]["correct"], false);
    let (_, a) =
        app.post(&ana, &format!("/api/v1/wrapped/answers/{id}"), json!({"question": "track", "chosen_rank": 1})).await;
    assert_eq!(a["correct"], true);
    assert_eq!(
        app.post(&ana, &format!("/api/v1/wrapped/answers/{id}"), json!({"question": "album", "chosen_rank": 9}))
            .await
            .0,
        400
    );
    assert_eq!(
        app.post(&bea, &format!("/api/v1/wrapped/answers/{id}"), json!({"question": "album", "chosen_rank": 1}))
            .await
            .0,
        404
    );

    // Foto fija: una escucha nueva de ese año no cambia el Wrapped, pero se marca como desactualizado.
    app.add(ana.id, "Zahara", "Berlín U5", "Puta", ly(2, 2, 0)).await;
    let (_, w2) = app.get(&ana, &format!("/api/v1/wrapped/{y}")).await;
    assert_eq!(w2["data"]["totals"]["plays"], 10);
    assert_eq!(w2["outdated"], true);
    assert_eq!(w2["answers"]["track"]["correct"], true);
    let (c, w3) = app.post(&ana, &format!("/api/v1/wrapped/{y}/regenerate"), json!({})).await;
    assert_eq!(c, 200, "{w3}");
    assert_eq!((w3["version"].as_i64(), w3["outdated"].as_bool()), (Some(2), Some(false)));
    assert_eq!(w3["data"]["totals"]["plays"], 11);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wrapped WHERE user_id = ? AND year = ?")
        .bind(ana.id)
        .bind(y)
        .fetch_one(&app.st.db)
        .await
        .unwrap();
    assert_eq!(n, 2, "la versión 1 se conserva");

    // Año en curso: vista previa.
    let (_, cur) = app.get(&ana, &format!("/api/v1/wrapped/{}", y + 1)).await;
    assert_eq!((cur["is_final"].as_bool(), cur["version"].as_i64()), (Some(false), Some(0)));
    assert_eq!(cur["data"]["totals"]["plays"], 1);

    assert_eq!(app.get(&ana, &format!("/api/v1/wrapped/{}", y - 5)).await.0, 404, "año sin escuchas");
    assert_eq!(app.get(&ana, &format!("/api/v1/wrapped/{}", y + 2)).await.0, 404, "año futuro");
    assert_eq!(app.get(&bea, &format!("/api/v1/wrapped/{y}")).await.0, 404, "bea no tiene escuchas");
}

#[tokio::test]
async fn audio_de_navidrome_con_range() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let bea = app.login("bea", "clave").await;
    app.add(ana.id, "Rosalía", "Saoko", "Motomami", Utc::now().timestamp() - 100).await;
    gordurasfm::enrich::pass(&app.st).await.unwrap();
    let url = format!("{}/api/v1/media/stream/s1", app.base);

    let r = app.http.get(&url).header("Cookie", &ana.cookie).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "audio/mpeg");
    assert!(r.headers().get("content-encoding").is_none(), "el audio no se comprime");
    assert_eq!(&r.bytes().await.unwrap()[..], MP3);

    let r = app
        .http
        .get(&url)
        .header("Cookie", &ana.cookie)
        .header("Range", "bytes=3-")
        .header("Accept-Encoding", "gzip")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert!(r.headers()["content-range"].to_str().unwrap().starts_with("bytes 3-"));
    assert_eq!(&r.bytes().await.unwrap()[..], &MP3[3..]);

    // Solo canciones que el usuario ha escuchado.
    assert_eq!(app.http.get(&url).header("Cookie", &bea.cookie).send().await.unwrap().status(), 404);
    let other = format!("{}/api/v1/media/stream/s9", app.base);
    assert_eq!(app.http.get(&other).header("Cookie", &ana.cookie).send().await.unwrap().status(), 404);
}

#[tokio::test]
async fn definitivo_automatico_del_año_anterior() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let _bea = app.login("bea", "clave").await;
    seed(&app, ana.id).await;
    assert_eq!(gordurasfm::wrapped::generate_due(&app.st).await.unwrap(), 1, "solo ana tiene escuchas");
    assert_eq!(gordurasfm::wrapped::generate_due(&app.st).await.unwrap(), 0, "no se repite");
    let (_, list) = app.get(&ana, "/api/v1/wrapped").await;
    let w = &list["years"][0]["wrapped"];
    assert_eq!(
        (w["is_final"].as_bool(), w["version"].as_i64(), w["outdated"].as_bool()),
        (Some(true), Some(1), Some(false))
    );
}
