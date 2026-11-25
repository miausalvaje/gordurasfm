//! Tests de integración de la fase 1: servidor real en un puerto libre, BD en memoria,
//! y Navidrome (Subsonic) y ListenBrainz falsos con wiremock.

use std::time::Duration;

use gordurasfm::{config::Config, crypto::Crypto, db, state};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{header, method, path},
};

const LB_TOKEN_ANA: &str = "lb-real-ana";

struct TestApp {
    base: String,
    http: reqwest::Client,
    lb: MockServer,
    _nd: MockServer,
}

/// Navidrome falso: usuarios ana/secreto y bea/clave. Comprueba t = md5(contraseña + s).
fn subsonic_responder(req: &Request) -> ResponseTemplate {
    let q: std::collections::HashMap<String, String> = req.url.query_pairs().into_owned().collect();
    let pass = match q.get("u").map(String::as_str) {
        Some("ana") => "secreto",
        Some("bea") => "clave",
        _ => "",
    };
    let ok = !pass.is_empty()
        && q.get("t") == Some(&gordurasfm::crypto::subsonic_token(pass, q.get("s").map(String::as_str).unwrap_or("")));
    let body = if !ok {
        json!({"subsonic-response": {"status": "failed", "version": "1.16.1", "error": {"code": 40, "message": "Wrong username or password"}}})
    } else if req.url.path().ends_with("/getUser") {
        json!({"subsonic-response": {"status": "ok", "version": "1.16.1", "user": {"username": q["u"], "adminRole": q["u"] == "ana"}}})
    } else {
        json!({"subsonic-response": {"status": "ok", "version": "1.16.1"}})
    };
    ResponseTemplate::new(200).set_body_json(body)
}

async fn spawn() -> TestApp {
    let nd = MockServer::start().await;
    Mock::given(method("GET")).respond_with(subsonic_responder).mount(&nd).await;

    let lb = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/1/validate-token"))
        .and(header("authorization", format!("Token {LB_TOKEN_ANA}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code": 200, "message": "Token valid.", "valid": true, "user_name": "ana_lb"})),
        )
        .with_priority(1)
        .mount(&lb)
        .await;
    Mock::given(method("GET"))
        .and(path("/1/validate-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code": 200, "message": "Token invalid.", "valid": false})),
        )
        .with_priority(5)
        .mount(&lb)
        .await;
    Mock::given(method("GET"))
        .and(path("/1/metadata/artist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"artist_mbid": "x", "rels": {}}])))
        .mount(&lb)
        .await;

    let mut cfg = Config::default();
    cfg.navidrome.url = nd.uri();
    cfg.listenbrainz.api_url = lb.uri();
    cfg.forward.base_delay_secs = 1;
    cfg.forward.max_delay_secs = 2;
    cfg.forward.poll_secs = 1;
    cfg.validate().unwrap();

    let pool = db::open_memory().await.unwrap();
    let st = state::new(cfg, pool, Crypto::from_key(&[9u8; 32]));
    gordurasfm::spawn_background(&st);
    let app = gordurasfm::app(st);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    TestApp { base: format!("http://{addr}"), http: gordurasfm::http_client::build(), lb, _nd: nd }
}

struct Session {
    cookie: String,
}

impl TestApp {
    async fn login(&self, user: &str, pass: &str) -> Session {
        let r = self
            .http
            .post(format!("{}/api/v1/auth/login", self.base))
            .header("X-Requested-With", "GordurasFM")
            .json(&json!({"username": user, "password": pass}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "login de {user}");
        let set = r.headers().get("set-cookie").unwrap().to_str().unwrap().to_string();
        assert!(set.contains("HttpOnly") && set.contains("SameSite=Lax"));
        Session { cookie: set.split(';').next().unwrap().to_string() }
    }

    async fn get(&self, s: &Session, p: &str) -> (u16, Value) {
        let r = self.http.get(format!("{}{p}", self.base)).header("Cookie", &s.cookie).send().await.unwrap();
        let st = r.status().as_u16();
        (st, r.json().await.unwrap_or(Value::Null))
    }

    async fn send(&self, s: &Session, m: reqwest::Method, p: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .request(m, format!("{}{p}", self.base))
            .header("Cookie", &s.cookie)
            .header("X-Requested-With", "GordurasFM")
            .json(&body)
            .send()
            .await
            .unwrap();
        let st = r.status().as_u16();
        (st, r.json().await.unwrap_or(Value::Null))
    }

    async fn scrobble_token(&self, s: &Session) -> String {
        let (st, v) = self
            .send(s, reqwest::Method::POST, "/api/v1/tokens", json!({"kind": "scrobble", "label": "Navidrome"}))
            .await;
        assert_eq!(st, 200);
        v["token"].as_str().unwrap().to_string()
    }

    /// Lo que mandaría Navidrome 0.64.2.
    async fn submit(&self, token: &str, listen_type: &str, track: &str, at: Option<i64>) -> reqwest::Response {
        let mut item = json!({"track_metadata": {
            "artist_name": "Rosalía", "track_name": track, "release_name": "Motomami",
            "additional_info": {"submission_client": "Navidrome", "submission_client_version": "0.64.2",
                                "duration_ms": 180000, "tracknumber": 1, "artist_names": ["Rosalía"]}}});
        if let Some(t) = at {
            item["listened_at"] = json!(t);
        }
        self.http
            .post(format!("{}/1/submit-listens", self.base))
            .header("Authorization", format!("Token {token}"))
            .json(&json!({"listen_type": listen_type, "payload": [item]}))
            .send()
            .await
            .unwrap()
    }

    async fn lb_submits(&self) -> Vec<Request> {
        self.lb.received_requests().await.unwrap().into_iter().filter(|r| r.url.path() == "/1/submit-listens").collect()
    }

    /// Espera (como mucho 10 s) a que el reenvío cumpla la condición.
    async fn wait_forward(&self, s: &Session, cond: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..100 {
            let (_, v) = self.get(s, "/api/v1/forward/status").await;
            if cond(&v) {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("el reenvío no llegó al estado esperado: {:?}", self.get(s, "/api/v1/forward/status").await.1);
    }
}

fn count(v: &Value, target: &str, status: &str) -> i64 {
    v.as_array()
        .unwrap()
        .iter()
        .find(|r| r["target"] == target && r["status"] == status)
        .and_then(|r| r["count"].as_i64())
        .unwrap_or(0)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[tokio::test]
async fn login_sesion_y_admin() {
    let app = spawn().await;
    // Sin la cabecera anti-CSRF
    let r = app
        .http
        .post(format!("{}/api/v1/auth/login", app.base))
        .json(&json!({"username": "ana", "password": "secreto"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    // Contraseña mala
    let r = app
        .http
        .post(format!("{}/api/v1/auth/login", app.base))
        .header("X-Requested-With", "x")
        .json(&json!({"username": "ana", "password": "mal"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    let ana = app.login("ana", "secreto").await;
    let (st, me) = app.get(&ana, "/api/v1/me").await;
    assert_eq!(st, 200);
    assert_eq!(me["username"], "ana");
    assert_eq!(me["is_admin"], true, "el primero que entra es admin");

    let bea = app.login("bea", "clave").await;
    let (_, me) = app.get(&bea, "/api/v1/me").await;
    assert_eq!(me["is_admin"], false);

    // Sin cookie no hay datos; con cookie, cambiar algo sin cabecera CSRF está prohibido.
    let r = app.http.get(format!("{}/api/v1/me", app.base)).send().await.unwrap();
    assert_eq!(r.status(), 401);
    let r = app
        .http
        .patch(format!("{}/api/v1/me", app.base))
        .header("Cookie", &ana.cookie)
        .json(&json!({"timezone": "UTC"}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403);
    let (st, me) =
        app.send(&ana, reqwest::Method::PATCH, "/api/v1/me", json!({"timezone": "America/Mexico_City"})).await;
    assert_eq!((st, me["timezone"].as_str()), (200, Some("America/Mexico_City")));

    // Logout invalida la sesión
    let (st, _) = app.send(&ana, reqwest::Method::POST, "/api/v1/auth/logout", json!({})).await;
    assert_eq!(st, 200);
    assert_eq!(app.get(&ana, "/api/v1/me").await.0, 401);
}

#[tokio::test]
async fn escuchas_desde_navidrome() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let tok = app.scrobble_token(&ana).await;

    // validate-token como lo hace Navidrome al enlazar
    let v: Value = app
        .http
        .get(format!("{}/1/validate-token", app.base))
        .header("Authorization", format!("Token {tok}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["valid"], true);
    assert_eq!(v["user_name"], "ana");

    // playing_now → «escuchando ahora», sin guardar escucha
    assert_eq!(app.submit(&tok, "playing_now", "Saoko", None).await.status(), 200);
    let (_, np) = app.get(&ana, "/api/v1/now-playing").await;
    assert_eq!(np["track"], "Saoko");

    let t = now() - 300;
    let r = app.submit(&tok, "single", "Saoko", Some(t)).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.json::<Value>().await.unwrap()["status"], "ok");
    // La misma escucha repetida (Navidrome reintenta) no se duplica
    assert_eq!(app.submit(&tok, "single", "Saoko", Some(t + 10)).await.status(), 200);
    assert_eq!(app.submit(&tok, "single", "Candy", Some(t + 200)).await.status(), 200);

    let (_, h) = app.get(&ana, "/api/v1/scrobbles").await;
    assert_eq!(h["total"], 2);
    assert_eq!(h["items"][0]["track"], "Candy");
    assert_eq!(h["items"][1]["album"], "Motomami");
    assert_eq!(h["items"][1]["source"], "navidrome");
    assert_eq!(app.get(&ana, "/api/v1/now-playing").await.1, Value::Null, "al guardar, se borra lo que sonaba");

    // Peticiones mal formadas → 400 como ListenBrainz
    let r = app
        .http
        .post(format!("{}/1/submit-listens", app.base))
        .header("Authorization", format!("Token {tok}"))
        .json(
            &json!({"listen_type": "single", "payload": [{"track_metadata": {"artist_name": "A", "track_name": "B"}}]}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);

    // Sin conexión de ListenBrainz no se encola nada
    assert!(app.lb_submits().await.is_empty());
}

#[tokio::test]
async fn reenvio_a_listenbrainz_con_reintento() {
    let app = spawn().await;
    // ListenBrainz falla una vez (503) y luego acepta.
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({"code": 503, "error": "down"})))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&app.lb)
        .await;
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .with_priority(5)
        .mount(&app.lb)
        .await;

    let ana = app.login("ana", "secreto").await;
    let tok = app.scrobble_token(&ana).await;
    let (st, c) =
        app.send(&ana, reqwest::Method::PUT, "/api/v1/connections/listenbrainz", json!({"token": LB_TOKEN_ANA})).await;
    assert_eq!(st, 200, "{c}");
    assert_eq!(c["remote_username"], "ana_lb");
    // Token falso → error claro
    let (st, _) =
        app.send(&ana, reqwest::Method::PUT, "/api/v1/connections/listenbrainz", json!({"token": "nada"})).await;
    assert_eq!(st, 400);

    assert_eq!(app.submit(&tok, "single", "Saoko", Some(now() - 100)).await.status(), 200);
    app.wait_forward(&ana, |v| count(v, "listenbrainz", "done") == 1).await;

    let subs = app.lb_submits().await;
    assert_eq!(subs.len(), 2, "un 503 y un reintento");
    let last = subs.last().unwrap();
    assert_eq!(last.headers.get("authorization").unwrap(), &format!("Token {LB_TOKEN_ANA}"));
    let body: Value = serde_json::from_slice(&last.body).unwrap();
    assert_eq!(body["listen_type"], "single");
    assert_eq!(body["payload"][0]["track_metadata"]["track_name"], "Saoko");
    assert_eq!(body["payload"][0]["track_metadata"]["additional_info"]["submission_client"], "Navidrome");

    // El token real de ListenBrainz de ana también vale como token de escucha (no hace falta cambiar Navidrome).
    assert_eq!(app.submit(LB_TOKEN_ANA, "single", "Candy", Some(now() - 50)).await.status(), 200);
    let (_, h) = app.get(&ana, "/api/v1/scrobbles").await;
    assert_eq!(h["total"], 2);
    app.wait_forward(&ana, |v| count(v, "listenbrainz", "done") == 2).await;

    // Desactivar el reenvío: lo nuevo queda en pausa; al reactivar, se envía.
    let (st, _) = app
        .send(&ana, reqwest::Method::PATCH, "/api/v1/connections/listenbrainz", json!({"forward_enabled": false}))
        .await;
    assert_eq!(st, 200);
    assert_eq!(app.submit(&tok, "single", "Hentai", Some(now() - 20)).await.status(), 200);
    let (_, h) = app.get(&ana, "/api/v1/scrobbles").await;
    assert_eq!(h["total"], 3);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (_, v) = app.get(&ana, "/api/v1/forward/status").await;
    assert_eq!(count(&v, "listenbrainz", "done"), 2, "con el reenvío desactivado no se envía nada nuevo");
    app.send(&ana, reqwest::Method::PATCH, "/api/v1/connections/listenbrainz", json!({"forward_enabled": true})).await;
    // Las desactivadas no se encolaron (el destino estaba apagado al llegar), así que siguen siendo 2.
    let (_, v) = app.get(&ana, "/api/v1/forward/status").await;
    assert_eq!(count(&v, "listenbrainz", "pending") + count(&v, "listenbrainz", "paused"), 0);
}

#[tokio::test]
async fn token_revocado_pausa_y_reconectar_reanuda() {
    let app = spawn().await;
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"code": 401, "error": "Invalid authorization token."})),
        )
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&app.lb)
        .await;
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status": "ok"})))
        .with_priority(5)
        .mount(&app.lb)
        .await;
    let ana = app.login("ana", "secreto").await;
    let tok = app.scrobble_token(&ana).await;
    app.send(&ana, reqwest::Method::PUT, "/api/v1/connections/listenbrainz", json!({"token": LB_TOKEN_ANA})).await;
    app.submit(&tok, "single", "Saoko", Some(now() - 100)).await;

    app.wait_forward(&ana, |v| count(v, "listenbrainz", "paused") == 1).await;
    let (_, c) = app.get(&ana, "/api/v1/connections").await;
    assert_eq!(c[0]["status"], "auth_failed");
    // Mientras está rota, lo nuevo también se guarda en local y queda en pausa.
    app.submit(&tok, "single", "Candy", Some(now() - 50)).await;
    let (_, v) = app.get(&ana, "/api/v1/forward/status").await;
    assert_eq!(count(&v, "listenbrainz", "paused"), 2);

    // Reconectar → se envía todo lo pendiente
    let (st, c) =
        app.send(&ana, reqwest::Method::PUT, "/api/v1/connections/listenbrainz", json!({"token": LB_TOKEN_ANA})).await;
    assert_eq!((st, c["status"].as_str()), (200, Some("ok")));
    app.wait_forward(&ana, |v| count(v, "listenbrainz", "done") == 2).await;
}

#[tokio::test]
async fn tokens_desconocidos_y_metadatos_pasan_a_listenbrainz() {
    let app = spawn().await;
    Mock::given(method("POST"))
        .and(path("/1/submit-listens"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"code": 401, "error": "Invalid authorization token."})),
        )
        .mount(&app.lb)
        .await;

    // Un usuario de Navidrome que no usa GordurasFM: su petición llega a ListenBrainz tal cual.
    let r = app.submit("token-de-otro", "single", "X", Some(now() - 10)).await;
    assert_eq!(r.status(), 401);
    let subs = app.lb_submits().await;
    assert_eq!(subs.len(), 1);
    assert_eq!(subs[0].headers.get("authorization").unwrap(), "Token token-de-otro");

    let v: Value = app
        .http
        .get(format!("{}/1/validate-token", app.base))
        .header("Authorization", "Token token-de-otro")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["valid"], false);

    // Navidrome pide metadatos de artistas a la misma BaseURL
    let r = app.http.get(format!("{}/1/metadata/artist?artist_mbids=x&inc=artist", app.base)).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let v: Value = r.json().await.unwrap();
    assert_eq!(v[0]["artist_mbid"], "x");
    let req =
        app.lb.received_requests().await.unwrap().into_iter().find(|r| r.url.path() == "/1/metadata/artist").unwrap();
    assert_eq!(req.url.query(), Some("artist_mbids=x&inc=artist"));
}

#[tokio::test]
async fn cada_usuario_ve_solo_lo_suyo() {
    let app = spawn().await;
    let ana = app.login("ana", "secreto").await;
    let bea = app.login("bea", "clave").await;
    let tok = app.scrobble_token(&ana).await;
    app.submit(&tok, "single", "Saoko", Some(now() - 100)).await;

    let (_, h) = app.get(&bea, "/api/v1/scrobbles").await;
    assert_eq!(h["total"], 0);
    let (_, toks) = app.get(&ana, "/api/v1/tokens").await;
    let id = toks[0]["id"].as_i64().unwrap();
    assert!(toks[0].get("token").is_none(), "el token no se vuelve a enseñar");
    let (st, _) = app.send(&bea, reqwest::Method::DELETE, &format!("/api/v1/tokens/{id}"), json!({})).await;
    assert_eq!(st, 404, "bea no puede borrar tokens de ana");
    let (st, _) = app.send(&ana, reqwest::Method::DELETE, &format!("/api/v1/tokens/{id}"), json!({})).await;
    assert_eq!(st, 200);
    // Token borrado → ya no es de nadie
    let v: Value = app
        .http
        .get(format!("{}/1/validate-token", app.base))
        .header("Authorization", format!("Token {tok}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["valid"], false);
}

#[tokio::test]
async fn salud_y_web() {
    let app = spawn().await;
    let v: Value = app.http.get(format!("{}/api/v1/salud", app.base)).send().await.unwrap().json().await.unwrap();
    assert_eq!(v["status"], "ok");
    // Rutas de la SPA devuelven index.html
    let r = app.http.get(format!("{}/wrapped/2026", app.base)).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.headers()["content-type"].to_str().unwrap().starts_with("text/html"));
    // Una ruta de API inexistente no devuelve la web
    assert_eq!(app.http.get(format!("{}/api/v1/nada", app.base)).send().await.unwrap().status(), 404);
}
