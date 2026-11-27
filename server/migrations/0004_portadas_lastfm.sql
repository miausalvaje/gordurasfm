-- Última vez que se buscó la portada de un álbum en Last.fm (para no insistir con los que no la tienen).
ALTER TABLE albums ADD COLUMN lf_checked_at INTEGER;
