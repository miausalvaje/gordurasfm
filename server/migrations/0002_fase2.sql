-- Fase 2: enriquecimiento con Navidrome, caché de imágenes, keys propias de Last.fm.

ALTER TABLE tracks  ADD COLUMN nd_checked_at INTEGER;
ALTER TABLE albums  ADD COLUMN nd_checked_at INTEGER;
ALTER TABLE artists ADD COLUMN nd_checked_at INTEGER;
CREATE INDEX tracks_nd_check  ON tracks(nd_checked_at);
CREATE INDEX albums_nd_check  ON albums(nd_checked_at);
CREATE INDEX artists_nd_check ON artists(nd_checked_at);

-- Imagen descargada para una referencia (nd:<coverArt> | url:<https…>) y tamaño.
-- hash NULL = no existe (caché negativa, se reintenta pasado un tiempo).
CREATE TABLE image_refs (
    ref        TEXT    NOT NULL,
    size       INTEGER NOT NULL,
    hash       TEXT,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (ref, size)
);

-- Key y secret de Last.fm propios de un usuario (opcional; si no, se usa la de la instancia).
CREATE TABLE lastfm_user_keys (
    user_id        INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    api_key        TEXT    NOT NULL,
    api_secret_enc BLOB    NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE INDEX scrobbles_user_source ON scrobbles(user_id, source, played_at);
CREATE INDEX jobs_user ON jobs(user_id, status);
