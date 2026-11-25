//! Normalización de nombres para agrupar (artistas, álbumes y canciones).
//! NFKC + minúsculas Unicode + espacios recortados y colapsados.
//! No se quitan «feat.» ni paréntesis, igual que Last.fm.

use unicode_normalization::UnicodeNormalization;

pub fn key(s: &str) -> String {
    let nfkc: String = s.nfkc().collect();
    let lower = nfkc.to_lowercase();
    lower.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Texto para mostrar: recorta y colapsa espacios, conserva mayúsculas.
pub fn display(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mayusculas_y_espacios() {
        assert_eq!(key("  Daft   PUNK "), "daft punk");
    }

    #[test]
    fn unicode_compatible() {
        // «ﬁ» (ligadura) y anchura completa se pliegan con NFKC.
        assert_eq!(key("ﬁesta"), "fiesta");
        assert_eq!(key("ＡＢＣ"), "abc");
        // É compuesta y descompuesta dan lo mismo.
        assert_eq!(key("Canci\u{00f3}n"), key("Cancio\u{0301}n"));
    }

    #[test]
    fn no_quita_feat() {
        assert_ne!(key("Tema (feat. X)"), key("Tema"));
    }
}
