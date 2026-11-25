-- Índice que cubre las consultas de estadísticas por periodo (incluida la duración): los periodos cortos
-- ya no recorren todo el historial del usuario. Sustituye a scrobbles_user_time.
CREATE INDEX scrobbles_user_time_cover ON scrobbles(user_id, played_at, track_id, album_id, artist_id, duration_ms);
DROP INDEX scrobbles_user_time;
