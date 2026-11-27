//! Tests de integración de la fase 2: estadísticas, Last.fm (importación, conexión, reenvío),
//! importación de ficheros, exportación, enriquecedor y portadas.

#![allow(clippy::field_reassign_with_default)]

use std::{collections::HashMap, time::Duration};

use chrono::{Datelike, TimeZone, Utc};
use ownfm::{
    AppState,
    config::Config,
    crypto::Crypto,
    db,
    scrobble::{self, Listen, Source},
    state,
};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path, query_param},
};

const LFM_KEY: &str = "0123456789abcdef0123456789abcdef";
const LFM_SECRET: &str = "fedcba9876543210fedcba9876543210";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000IHDRfalso";

struct TestApp {
    base: String,
    http: reqwest::Client,
    st: AppState,
    nd: MockServer,
    lfm: MockServer,
}

fn subsonic(req: &Request) -> ResponseTemplate {
    let q: HashMap<String, String> = req.url.query_pairs().into_owned().collect();
    let pass = match q.get("u").map(String::as_str) {
        Some("ana") => "secreto",
        Some("bea") => "clave",
        _ => "",
    };
    let ok = !pass.is_empty()
        && q.get("t") == Some(&ownfm::crypto::subsonic_token(pass, q.get("s").map(String::as_str).unwrap_or("")));
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
    } else {
        wrap(json!({}))
    }
}

async fn spawn() -> TestApp {
    spawn_with(true).await
}

/// `secret = false`: la instancia solo tiene la API key de Last.fm.
async fn spawn_with(secret: bool) -> TestApp {
    let nd = MockServer::start().await;
    Mock::given(method("GET")).respond_with(subsonic).mount(&nd).await;
    let lfm = MockServer::start().await;
    Mock::given(method("GET"))
        .and(query_param("method", "user.getInfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"user": {"name": "AnaLfm"}})))
        .mount(&lfm)
        .await;

    let data = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.data_dir = data.keep();
    cfg.navidrome.url = nd.uri();
    cfg.listenbrainz.api_url = "http://127.0.0.1:9".into();
    cfg.lastfm.api_url = format!("{}/2.0/", lfm.uri());
    cfg.lastfm.api_key = Some(LFM_KEY.into());
    cfg.lastfm.api_secret = secret.then(|| LFM_SECRET.into());
    cfg.lastfm.import_delay_ms = 0;
    cfg.enrich.enabled = false; // se llama a mano
    cfg.forward.base_delay_secs = 1;
    cfg.forward.max_delay_secs = 2;
    cfg.forward.poll_secs = 1;
    cfg.validate().unwrap();

    let pool = db::open_memory().await.unwrap();
    let st = state::new(cfg, pool, Crypto::from_key(&[5u8; 32]));
    ownfm::spawn_background(&st);
    let app = ownfm::app(st.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    TestApp { base: format!("http://{addr}"), http: ownfm::http_client::build(), st, nd, lfm }
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

    async fn send(&self, s: &S, m: reqwest::Method, p: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .request(m, format!("{}{p}", self.base))
            .header("Cookie", &s.cookie)
            .header("X-Requested-With", "t")
            .json(&body)
            .send()
            .await
            .unwrap();
        let code = r.status().as_u16();
        (code, r.json().await.unwrap_or(Value::Null))
    }

    async fn upload(&self, s: &S, name: &str, content: &str) -> (u16, Value) {
        let b = "----ownfmlimite";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: text/plain\r\n\r\n{content}\r\n--{b}--\r\n"
        );
        let r = self
            .http
            .post(format!("{}/api/v1/import/file", self.base))
            .header("Cookie", &s.cookie)
            .header("X-Requested-With", "t")
            .header("Content-Type", format!("multipart/form-data; boundary={b}"))
            .body(body)
            .send()
            .await
            .unwrap();
        let code = r.status().as_u16();
        (code, r.json().await.unwrap_or(Value::Null))
    }

    async fn wait_job(&self, s: &S, id: i64) -> Value {
        for _ in 0..150 {
            let (_, j) = self.get(s, &format!("/api/v1/import/jobs/{id}")).await;
            if matches!(j["status"].as_str(), Some("done" | "failed" | "cancelled")) {
                return j;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("el trabajo {id} no terminó");
    }

    async fn add(&self, user: i64, artist: &str, track: &str, album: &str, at: i64, dur: Option<i64>) {
        let l = Listen {
            artist: artist.into(),
            track: track.into(),
            album: Some(album.into()),
            album_artist: None,
            duration_ms: dur,
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

/// Mediodía (hora de Madrid) del día y mes indicados del año pasado.
fn last_year(month: u32, day: u32) -> i64 {
    let y = Utc::now().year() - 1;
    chrono_tz::Europe::Madrid.with_ymd_and_hms(y, month, day, 12, 0, 0).unwrap().timestamp()
}

#[tokio::test]
async fn estadisticas_por_periodo_y_tops() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    for (m, d) in [(1, 10), (3, 5), (3, 6)] {
        app.add(ana.id, "Rosalía", "Saoko", "Motomami", last_year(m, d), Some(137_000)).await;
    }
    app.add(ana.id, "Rosalía", "Candy", "Motomami", last_year(3, 7), Some(193_000)).await;
    app.add(ana.id, "Rosalía", "Candy", "Motomami", last_year(7, 1), Some(193_000)).await;
    app.add(ana.id, "Bad Gyal", "Fiebre", "Slow Wine", last_year(7, 2), None).await;
    app.add(ana.id, "Bad Gyal", "Fiebre", "Slow Wine", last_year(12, 31), None).await;
    app.add(ana.id, "Rosalía", "Saoko", "Motomami", Utc::now().timestamp() - 3600, Some(137_000)).await;

    let (code, v) = app.get(&ana, "/api/v1/stats?period=last_year").await;
    assert_eq!(code, 200, "{v}");
    let t = &v["totals"];
    assert_eq!(t["plays"], 7);
    assert_eq!((t["artists"].as_i64(), t["albums"].as_i64(), t["tracks"].as_i64()), (Some(2), Some(2), Some(3)));
    assert_eq!(t["listening_ms"], 3 * 137_000 + 2 * 193_000);
    assert_eq!(t["plays_without_duration"], 2);
    assert_eq!(v["period"]["unit"], "month");
    let chart = v["chart"].as_array().unwrap();
    assert_eq!(chart.len(), 12);
    assert_eq!(chart.iter().map(|c| c["plays"].as_i64().unwrap()).sum::<i64>(), 7);
    assert_eq!(chart[2]["plays"], 3, "marzo");
    assert_eq!(chart[11]["plays"], 1, "el 31 de diciembre a mediodía es del año pasado");
    assert_eq!(t["by_hour"][12], 7, "todas a las 12 hora local");

    assert_eq!(v["top_albums"][0]["title"], "Motomami");
    assert_eq!(v["top_albums"][0]["plays"], 5);
    assert_eq!(v["top_tracks"][0]["title"], "Saoko");
    assert_eq!(v["top_tracks"][0]["album"], "Motomami");
    assert_eq!(v["top_tracks"][0]["rank"], 1);
    assert_eq!(
        (v["top_tracks"][1]["rank"].as_i64(), v["top_tracks"][2]["rank"].as_i64()),
        (Some(2), Some(2)),
        "empate"
    );
    assert_eq!(v["top_artists"][0]["name"], "Rosalía");
    assert_eq!(v["top_artists"][0]["plays"], 5);

    // Paginación: el tercero empata con el segundo aunque esté en otra página.
    let (_, p2) = app.get(&ana, "/api/v1/top/tracks?period=last_year&limit=2&page=2").await;
    assert_eq!(p2["total"], 3);
    assert_eq!(p2["items"][0]["rank"], 2);

    let (_, all) = app.get(&ana, "/api/v1/stats?period=all_time").await;
    assert_eq!(all["totals"]["plays"], 8);
    let (_, ty) = app.get(&ana, "/api/v1/stats?period=this_year").await;
    assert_eq!(ty["totals"]["plays"], 1);

    // La caché no devuelve datos viejos tras una escucha nueva.
    app.add(ana.id, "Bad Gyal", "Fiebre", "Slow Wine", last_year(8, 8), None).await;
    let (_, v2) = app.get(&ana, "/api/v1/stats?period=last_year").await;
    assert_eq!(v2["totals"]["plays"], 8);

    assert_eq!(app.get(&ana, "/api/v1/stats?period=ayer").await.0, 400);
    assert_eq!(app.get(&ana, "/api/v1/top/generos").await.0, 404);
    // Otro usuario no ve nada de ana
    let bea = app.login("bea", "clave").await;
    let (_, b) = app.get(&bea, "/api/v1/stats?period=all_time").await;
    assert_eq!(b["totals"]["plays"], 0);
}

fn recent(tracks: Vec<Value>, page: i64, total_pages: i64) -> Value {
    json!({"recenttracks": {"track": tracks, "@attr": {"user": "AnaLfm", "page": page.to_string(), "totalPages": total_pages.to_string(), "perPage": "200", "total": "3"}}})
}

fn lfm_track(artist: &str, name: &str, album: &str, uts: i64) -> Value {
    json!({"artist": {"#text": artist, "mbid": ""}, "name": name, "album": {"#text": album, "mbid": ""}, "mbid": "",
           "image": [{"size": "extralarge", "#text": "https://lastfm.freetls.fastly.net/i/u/300x300/abc.png"}],
           "date": {"uts": uts.to_string(), "#text": ""}})
}

#[tokio::test]
async fn importar_de_lastfm_reanudable_y_solo_lo_nuevo() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let now = Utc::now().timestamp();
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(recent(
            vec![
                json!({"artist": {"#text": "X"}, "name": "Sonando", "@attr": {"nowplaying": "true"}}),
                lfm_track("Rosalía", "Saoko", "Motomami", now - 1000),
                lfm_track("Rosalía", "Candy", "Motomami", now - 2000),
            ],
            1,
            2,
        )))
        .mount(&app.lfm)
        .await;
    // La página 2 falla una vez (límite de peticiones) y luego responde.
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": 29, "message": "Rate limit exceeded"})))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&app.lfm)
        .await;
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(recent(
            vec![lfm_track("Bad Gyal", "Fiebre", "Slow Wine", now - 90_000)],
            2,
            2,
        )))
        .with_priority(5)
        .mount(&app.lfm)
        .await;

    let (code, job) = app
        .send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"mode": "full", "username": "analfm"}))
        .await;
    assert_eq!(code, 200, "{job}");
    // Mientras corre, no se puede lanzar otra
    let (code, _) = app.send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"username": "analfm"})).await;
    assert_eq!(code, 400);
    let j = app.wait_job(&ana, job["id"].as_i64().unwrap()).await;
    assert_eq!(j["status"], "done", "{j}");
    assert_eq!(j["stats"]["stored"], 3);
    assert_eq!(j["stats"]["invalid"], 1, "la que suena ahora no cuenta");
    assert_eq!((j["progress_done"].as_i64(), j["progress_total"].as_i64()), (Some(2), Some(2)));

    let (_, h) = app.get(&ana, "/api/v1/scrobbles").await;
    assert_eq!(h["total"], 3);
    assert_eq!(h["items"][0]["source"], "lastfm");
    // Las portadas de Last.fm quedan como referencia del álbum
    let (_, s) = app.get(&ana, "/api/v1/stats?period=all_time").await;
    assert!(s["top_albums"][0]["cover"].as_str().unwrap().starts_with("/api/v1/media/cover/"));

    // Importar todo otra vez: todo duplicado
    let (_, job2) = app
        .send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"mode": "full", "username": "analfm"}))
        .await;
    let j2 = app.wait_job(&ana, job2["id"].as_i64().unwrap()).await;
    assert_eq!((j2["stats"]["stored"].as_i64(), j2["stats"]["duplicates"].as_i64()), (Some(0), Some(3)));

    // «Solo lo nuevo» pide desde la última escucha importada menos 1 h
    let (_, job3) = app
        .send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"mode": "new", "username": "analfm"}))
        .await;
    app.wait_job(&ana, job3["id"].as_i64().unwrap()).await;
    let reqs = app.lfm.received_requests().await.unwrap();
    let last = reqs.iter().rev().find(|r| r.url.query().unwrap_or("").contains("getRecentTracks")).unwrap();
    let q: HashMap<String, String> = last.url.query_pairs().into_owned().collect();
    assert_eq!(q["from"], (now - 1000 - 3600).to_string());

    // Usuario inexistente en Last.fm → error claro
    Mock::given(method("GET"))
        .and(query_param("method", "user.getInfo"))
        .and(query_param("user", "nadie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": 6, "message": "User not found"})))
        .with_priority(1)
        .mount(&app.lfm)
        .await;
    let (code, _) = app.send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"username": "nadie"})).await;
    assert_eq!(code, 400);
}

#[tokio::test]
async fn importar_lastfm_solo_con_key() {
    let app = spawn_with(false).await;
    let ana = app.login("ana", "secreto").await;
    let now = chrono::Utc::now().timestamp();
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(recent(
            vec![
                lfm_track("Rosalía", "Saoko", "Motomami", now - 5000),
                lfm_track("Bad Gyal", "Fiebre", "Slow Wine", now - 4000),
            ],
            1,
            1,
        )))
        .mount(&app.lfm)
        .await;
    let (_, st) = app.get(&ana, "/api/v1/lastfm/status").await;
    assert_eq!(st["available"], true, "{st}");
    // Importar funciona sin secret…
    let (code, job) = app
        .send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"mode": "full", "username": "analfm"}))
        .await;
    assert_eq!(code, 200, "{job}");
    let j = app.wait_job(&ana, job["id"].as_i64().unwrap()).await;
    assert_eq!(j["status"], "done", "{j}");
    assert_eq!(j["stats"]["stored"], 2);
    // …pero conectar la cuenta no, y el error lo explica.
    let (code, e) = app.get(&ana, "/api/v1/connections/lastfm/start").await;
    assert_eq!(code, 400);
    assert!(e["message"].as_str().unwrap().contains("secret"), "{e}");
}

#[tokio::test]
async fn conectar_lastfm_y_reenviar() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    Mock::given(method("GET"))
        .and(query_param("method", "auth.getSession"))
        .and(query_param("token", "TOKEN-WEB"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"session": {"name": "AnaLfm", "key": "SESSIONKEY", "subscriber": 0}})),
        )
        .mount(&app.lfm)
        .await;
    Mock::given(method("POST"))
        .and(path("/2.0/"))
        .respond_with(|req: &Request| {
            let form: HashMap<String, String> = url::form_urlencoded::parse(&req.body).into_owned().collect();
            let n = form.keys().filter(|k| k.starts_with("timestamp[")).count();
            let items: Vec<Value> = (0..n).map(|_| json!({"ignoredMessage": {"code": "0", "#text": ""}})).collect();
            ResponseTemplate::new(200)
                .set_body_json(json!({"scrobbles": {"scrobble": items, "@attr": {"accepted": n, "ignored": 0}}}))
        })
        .mount(&app.lfm)
        .await;

    let (code, start) = app.get(&ana, "/api/v1/connections/lastfm/start").await;
    assert_eq!(code, 200);
    let url = start["url"].as_str().unwrap();
    assert!(url.contains(&format!("api_key={LFM_KEY}")));
    let cb = url::Url::parse(url).unwrap().query_pairs().find(|(k, _)| k == "cb").unwrap().1.to_string();
    let state = url::Url::parse(&cb).unwrap().query_pairs().find(|(k, _)| k == "state").unwrap().1.to_string();

    // Estado falso → error
    let r = app
        .http
        .get(format!("{}/api/v1/connections/lastfm/callback?state=malo&token=TOKEN-WEB", app.base))
        .header("Cookie", &ana.cookie)
        .send()
        .await
        .unwrap();
    assert!(r.url().query().unwrap().contains("lastfm=error"));
    // Vuelta buena desde Last.fm
    let r = app
        .http
        .get(format!("{}/api/v1/connections/lastfm/callback?state={state}&token=TOKEN-WEB", app.base))
        .header("Cookie", &ana.cookie)
        .send()
        .await
        .unwrap();
    assert!(r.url().query().unwrap().contains("lastfm=ok"), "{}", r.url());
    let (_, c) = app.get(&ana, "/api/v1/connections").await;
    assert_eq!(c[0]["provider"], "lastfm");
    assert_eq!(c[0]["remote_username"], "AnaLfm");
    assert_eq!(c[0]["forward_enabled"], false, "el reenvío a Last.fm empieza apagado");

    // Activar reenvío y mandar escuchas desde «Navidrome»
    app.send(&ana, reqwest::Method::PATCH, "/api/v1/connections/lastfm", json!({"forward_enabled": true})).await;
    let (_, tok) = app.send(&ana, reqwest::Method::POST, "/api/v1/tokens", json!({"kind": "scrobble"})).await;
    let tok = tok["token"].as_str().unwrap().to_string();
    let now = Utc::now().timestamp();
    for (track, at) in [("Saoko", now - 300), ("Viejísima", now - 20 * 86400)] {
        let r = app
            .http
            .post(format!("{}/1/submit-listens", app.base))
            .header("Authorization", format!("Token {tok}"))
            .json(&json!({"listen_type": "single", "payload": [{"listened_at": at, "track_metadata": {
                "artist_name": "Rosalía", "track_name": track, "release_name": "Motomami", "additional_info": {"duration_ms": 137000}}}]}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }
    let mut status = Value::Null;
    for _ in 0..60 {
        status = app.get(&ana, "/api/v1/forward/status").await.1;
        let n = |s: &str| {
            status
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["target"] == "lastfm" && r["status"] == s)
                .map(|r| r["count"].as_i64().unwrap())
                .unwrap_or(0)
        };
        if n("done") == 1 && n("dead") == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let reqs: Vec<Request> =
        app.lfm.received_requests().await.unwrap().into_iter().filter(|r| r.method.as_str() == "POST").collect();
    assert_eq!(reqs.len(), 1, "solo se envía la reciente; estado: {status}");
    let form: HashMap<String, String> = url::form_urlencoded::parse(&reqs[0].body).into_owned().collect();
    assert_eq!(form["method"], "track.scrobble");
    assert_eq!(form["sk"], "SESSIONKEY");
    assert_eq!(form["track[0]"], "Saoko");
    assert_eq!(form["duration[0]"], "137");
    // Firma correcta
    let mut params: std::collections::BTreeMap<String, String> = form.clone().into_iter().collect();
    let sig = params.remove("api_sig").unwrap();
    assert_eq!(sig, ownfm::lastfm::client::sign(&params, LFM_SECRET));
}

#[tokio::test]
async fn importar_fichero_y_exportar() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    app.add(ana.id, "Rosalía", "Saoko", "Motomami", 1_700_000_000, None).await;
    let csv = "Rosalía,Motomami,Saoko,14 Nov 2023 22:13\nRosalía,Motomami,Candy,14 Nov 2023 22:20\n\"Bad Gyal\",Slow Wine,Fiebre,15 Nov 2023 10:00\nsin fecha,,x,\n";
    let (code, r) = app.upload(&ana, "historial.csv", csv).await;
    assert_eq!(code, 200, "{r}");
    assert_eq!((r["format"].as_str(), r["listens"].as_i64(), r["invalid"].as_i64()), (Some("csv"), Some(3), Some(1)));
    let j = app.wait_job(&ana, r["job"]["id"].as_i64().unwrap()).await;
    assert_eq!(j["status"], "done");
    assert_eq!((j["stats"]["stored"].as_i64(), j["stats"]["duplicates"].as_i64()), (Some(2), Some(1)));

    // Exportar y volver a importar: todo duplicado
    let r = app
        .http
        .get(format!("{}/api/v1/export?format=csv", app.base))
        .header("Cookie", &ana.cookie)
        .send()
        .await
        .unwrap();
    assert!(r.headers()["content-disposition"].to_str().unwrap().contains("ownfm-ana-"));
    let exported = r.text().await.unwrap();
    assert!(exported.starts_with("played_at,artist,album,track,duration_ms,source\n"));
    assert_eq!(exported.lines().count(), 4);
    let (_, r2) = app.upload(&ana, "export.csv", &exported).await;
    let j2 = app.wait_job(&ana, r2["job"]["id"].as_i64().unwrap()).await;
    assert_eq!((j2["stats"]["stored"].as_i64(), j2["stats"]["duplicates"].as_i64()), (Some(0), Some(3)));

    let json_export: Value = app
        .http
        .get(format!("{}/api/v1/export", app.base))
        .header("Cookie", &ana.cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(json_export.as_array().unwrap().len(), 3);

    // Ficheros que no valen
    assert_eq!(app.upload(&ana, "x.json", "[{\"nada\": 1}]").await.0, 400);
    assert_eq!(app.upload(&ana, "x.json", "").await.0, 400);
}

#[tokio::test]
async fn enriquecedor_y_portadas_en_cache() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let bea = app.login("bea", "clave").await;
    app.add(ana.id, "Rosalía", "Saoko", "Motomami", Utc::now().timestamp() - 100, None).await;
    app.add(bea.id, "Otro", "Tema", "Disco", Utc::now().timestamp() - 100, None).await;

    let n = ownfm::enrich::pass(&app.st).await.unwrap();
    assert!(n >= 3);
    let (_, s) = app.get(&ana, "/api/v1/stats?period=this_week").await;
    let al = &s["top_albums"][0];
    let cover = al["cover"].as_str().expect("portada tras enriquecer").to_string();
    assert_eq!(s["top_artists"][0]["image"], format!("/api/v1/media/artist/{}", s["top_artists"][0]["id"]));
    assert_eq!(s["top_tracks"][0]["duration_ms"], 137_000, "duración rellenada desde Navidrome");
    let (sid,): (String,) =
        sqlx::query_as("SELECT navidrome_song_id FROM track_navidrome").fetch_one(&app.st.db).await.unwrap();
    assert_eq!(sid, "s1");

    for _ in 0..2 {
        let r = app.http.get(format!("{}{cover}", app.base)).header("Cookie", &ana.cookie).send().await.unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(r.headers()["content-type"], "image/png");
        assert_eq!(&r.bytes().await.unwrap()[..], PNG);
    }
    let cover_calls =
        app.nd.received_requests().await.unwrap().iter().filter(|r| r.url.path().ends_with("/getCoverArt")).count();
    assert_eq!(cover_calls, 1, "la segunda vez sale de la caché");
    // bea no ha escuchado ese álbum
    let r = app.http.get(format!("{}{cover}", app.base)).header("Cookie", &bea.cookie).send().await.unwrap();
    assert_eq!(r.status(), 404);
    // Sin sesión, nada
    assert_eq!(app.http.get(format!("{}{cover}", app.base)).send().await.unwrap().status(), 401);
}

#[tokio::test]
async fn administracion() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let bea = app.login("bea", "clave").await;
    assert_eq!(app.get(&bea, "/api/v1/admin/users").await.0, 403);
    let (code, users) = app.get(&ana, "/api/v1/admin/users").await;
    assert_eq!(code, 200);
    assert_eq!(users.as_array().unwrap().len(), 2);
    let (_, lf) = app.get(&ana, "/api/v1/admin/lastfm").await;
    assert_eq!((lf["configured"].as_bool(), lf["source"].as_str()), (Some(true), Some("config")));
    // Quitar la aprobación cierra sus sesiones
    let (code, _) = app
        .send(&ana, reqwest::Method::PATCH, &format!("/api/v1/admin/users/{}", bea.id), json!({"is_approved": false}))
        .await;
    assert_eq!(code, 200);
    assert_eq!(app.get(&bea, "/api/v1/me").await.0, 401);
    let (code, _) = app
        .send(&ana, reqwest::Method::PATCH, &format!("/api/v1/admin/users/{}", ana.id), json!({"is_admin": false}))
        .await;
    assert_eq!(code, 400);
}

#[tokio::test]
async fn importacion_fallida_se_reanuda_por_donde_iba() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let now = Utc::now().timestamp();
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(recent(
            vec![lfm_track("A", "Uno", "Disco", now - 100)],
            1,
            2,
        )))
        .mount(&app.lfm)
        .await;
    // Error que no se arregla reintentando (key suspendida): el trabajo se para.
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": 26, "message": "Suspended API key"})))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&app.lfm)
        .await;
    Mock::given(method("GET"))
        .and(query_param("method", "user.getRecentTracks"))
        .and(query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(recent(
            vec![lfm_track("A", "Dos", "Disco", now - 200)],
            2,
            2,
        )))
        .with_priority(5)
        .mount(&app.lfm)
        .await;

    let (_, job) = app.send(&ana, reqwest::Method::POST, "/api/v1/import/lastfm", json!({"username": "analfm"})).await;
    let id = job["id"].as_i64().unwrap();
    let j = app.wait_job(&ana, id).await;
    assert_eq!(j["status"], "failed");
    assert!(j["error"].as_str().unwrap().contains("Suspended"));
    assert_eq!(j["stats"]["stored"], 1);
    assert_eq!(j["progress_done"], 1);

    let (code, _) = app.send(&ana, reqwest::Method::POST, &format!("/api/v1/import/jobs/{id}/resume"), json!({})).await;
    assert_eq!(code, 200);
    let j = app.wait_job(&ana, id).await;
    assert_eq!(j["status"], "done", "{j}");
    assert_eq!(j["stats"]["stored"], 2);
    let page1 = app
        .lfm
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| {
            r.url.query().unwrap_or("").contains("getRecentTracks") && r.url.query().unwrap_or("").contains("page=1&")
        })
        .count();
    assert_eq!(page1, 1, "al reanudar no se vuelve a pedir la página 1");
}
