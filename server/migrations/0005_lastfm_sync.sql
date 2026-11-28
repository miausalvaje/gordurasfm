-- Sincronización periódica con Last.fm: la fila existe = el usuario la tiene activada.
CREATE TABLE lastfm_sync (
    user_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    username    TEXT    NOT NULL,   -- usuario de Last.fm
    synced_to   INTEGER NOT NULL,   -- hasta cuándo (Unix) se trajo lo último con éxito
    last_run_at INTEGER,
    last_error  TEXT,
    created_at  INTEGER NOT NULL
);
