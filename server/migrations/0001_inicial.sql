-- GordurasFM: esquema inicial. Tiempos en segundos Unix (UTC).

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT    NOT NULL UNIQUE,          -- usuario de Navidrome
    display_name  TEXT    NOT NULL,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    is_approved   INTEGER NOT NULL DEFAULT 1,
    timezone      TEXT    NOT NULL DEFAULT 'Europe/Madrid',
    is_public     INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    last_login_at INTEGER
);

-- (token, salt) de Subsonic cifrados. Nunca la contraseña.
CREATE TABLE navidrome_credentials (
    user_id    INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_enc BLOB    NOT NULL,
    status     TEXT    NOT NULL DEFAULT 'ok',       -- ok | invalid
    updated_at INTEGER NOT NULL
);

CREATE TABLE sessions (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash   BLOB    NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    user_agent   TEXT
);
CREATE INDEX sessions_user ON sessions(user_id);

-- kind = 'scrobble' (el que se pega en Navidrome) | 'api' (integraciones)
CREATE TABLE api_tokens (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL,
    token_hash   BLOB    NOT NULL UNIQUE,
    hint         TEXT    NOT NULL,                  -- últimos 4 caracteres, para reconocerlo
    label        TEXT,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX api_tokens_user ON api_tokens(user_id);

-- provider = 'listenbrainz' | 'lastfm'
CREATE TABLE connections (
    user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider           TEXT    NOT NULL,
    remote_username    TEXT,
    secret_enc         BLOB    NOT NULL,             -- token de LB o session key de Last.fm
    lookup_hmac        BLOB,                         -- HMAC del token de LB (para reconocerlo)
    own_api_key        TEXT,
    own_api_secret_enc BLOB,
    forward_enabled    INTEGER NOT NULL DEFAULT 1,
    status             TEXT    NOT NULL DEFAULT 'ok', -- ok | auth_failed
    last_error         TEXT,
    updated_at         INTEGER NOT NULL,
    PRIMARY KEY (user_id, provider)
);
CREATE UNIQUE INDEX connections_lookup ON connections(lookup_hmac) WHERE lookup_hmac IS NOT NULL;

CREATE TABLE artists (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    name_key     TEXT NOT NULL UNIQUE,
    mbid         TEXT,
    navidrome_id TEXT,
    image_ref    TEXT
);

CREATE TABLE albums (
    id           INTEGER PRIMARY KEY,
    title        TEXT    NOT NULL,
    title_key    TEXT    NOT NULL,
    artist_id    INTEGER NOT NULL REFERENCES artists(id),
    mbid         TEXT,
    navidrome_id TEXT,
    cover_ref    TEXT,
    UNIQUE (artist_id, title_key)
);

CREATE TABLE tracks (
    id          INTEGER PRIMARY KEY,
    title       TEXT    NOT NULL,
    title_key   TEXT    NOT NULL,
    artist_id   INTEGER NOT NULL REFERENCES artists(id),
    duration_ms INTEGER,
    mbid        TEXT,
    UNIQUE (artist_id, title_key)
);

CREATE TABLE track_navidrome (
    track_id          INTEGER NOT NULL REFERENCES tracks(id),
    album_id          INTEGER NOT NULL REFERENCES albums(id),
    navidrome_song_id TEXT    NOT NULL,
    PRIMARY KEY (track_id, album_id)
);

CREATE TABLE scrobbles (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id    INTEGER NOT NULL REFERENCES tracks(id),
    album_id    INTEGER REFERENCES albums(id),
    artist_id   INTEGER NOT NULL REFERENCES artists(id),
    played_at   INTEGER NOT NULL,
    duration_ms INTEGER,
    source      TEXT    NOT NULL,   -- navidrome | navidrome_poll | lastfm | import_file
    client      TEXT,
    raw         TEXT,               -- JSON original
    created_at  INTEGER NOT NULL
);
CREATE INDEX scrobbles_user_time   ON scrobbles(user_id, played_at);
CREATE INDEX scrobbles_user_track  ON scrobbles(user_id, track_id, played_at);
CREATE INDEX scrobbles_user_album  ON scrobbles(user_id, album_id, played_at);
CREATE INDEX scrobbles_user_artist ON scrobbles(user_id, artist_id, played_at);

CREATE TABLE forward_queue (
    id              INTEGER PRIMARY KEY,
    scrobble_id     INTEGER NOT NULL REFERENCES scrobbles(id) ON DELETE CASCADE,
    user_id         INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target          TEXT    NOT NULL,   -- listenbrainz | lastfm
    status          TEXT    NOT NULL,   -- pending | done | failed | dead | paused
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL,
    last_error      TEXT,
    updated_at      INTEGER NOT NULL,
    UNIQUE (scrobble_id, target)
);
CREATE INDEX forward_due ON forward_queue(status, next_attempt_at);
CREATE INDEX forward_user ON forward_queue(user_id, target, status);

CREATE TABLE jobs (
    id             INTEGER PRIMARY KEY,
    user_id        INTEGER REFERENCES users(id) ON DELETE CASCADE,
    kind           TEXT    NOT NULL,
    status         TEXT    NOT NULL,   -- queued | running | done | failed | cancelled
    cursor         TEXT,
    progress_done  INTEGER NOT NULL DEFAULT 0,
    progress_total INTEGER,
    stats          TEXT,
    error          TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE TABLE wrapped (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    year         INTEGER NOT NULL,
    version      INTEGER NOT NULL,
    is_final     INTEGER NOT NULL,
    generated_at INTEGER NOT NULL,
    data         TEXT    NOT NULL,
    UNIQUE (user_id, year, version)
);

CREATE TABLE wrapped_answers (
    wrapped_id  INTEGER NOT NULL REFERENCES wrapped(id) ON DELETE CASCADE,
    question    TEXT    NOT NULL,
    chosen_rank INTEGER NOT NULL,
    correct     INTEGER NOT NULL,
    answered_at INTEGER NOT NULL,
    PRIMARY KEY (wrapped_id, question)
);

CREATE TABLE images (
    hash       TEXT PRIMARY KEY,
    mime       TEXT NOT NULL,
    width      INTEGER,
    height     INTEGER,
    source     TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
