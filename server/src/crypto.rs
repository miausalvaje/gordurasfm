//! Cifrado de secretos guardados (XChaCha20-Poly1305), HMAC de búsqueda y tokens aleatorios.

use std::path::Path;

use anyhow::{Context, Result, bail};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use hmac::{Mac, SimpleHmac};
use sha2::{Digest, Sha256};

const VERSION: u8 = 1;

#[derive(Clone)]
pub struct Crypto {
    cipher: XChaCha20Poly1305,
    hmac_key: [u8; 32],
}

impl Crypto {
    pub fn from_key(key: &[u8; 32]) -> Self {
        let cipher = XChaCha20Poly1305::new_from_slice(key).expect("clave de 32 bytes");
        // Clave distinta para el HMAC, derivada de la maestra.
        let mut h = Sha256::new();
        h.update(b"ownfm-hmac-v1");
        h.update(key);
        let hmac_key: [u8; 32] = h.finalize().into();
        Self { cipher, hmac_key }
    }

    /// Carga la clave del archivo o la crea (permisos 600) si no existe.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            let txt = std::fs::read_to_string(path).with_context(|| format!("leyendo {}", path.display()))?;
            let bytes = hex::decode(txt.trim()).context("la clave maestra no es hex")?;
            let key: [u8; 32] =
                bytes.try_into().map_err(|_| anyhow::anyhow!("la clave maestra debe tener 32 bytes"))?;
            return Ok(Self::from_key(&key));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut key = [0u8; 32];
        rand::fill(&mut key);
        write_private(path, hex::encode(key).as_bytes())?;
        tracing::info!(ruta = %path.display(), "clave maestra creada");
        Ok(Self::from_key(&key))
    }

    pub fn encrypt(&self, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 24];
        rand::fill(&mut nonce);
        let ct = self.cipher.encrypt(&XNonce::from(nonce), plain).expect("cifrado");
        let mut out = Vec::with_capacity(1 + 24 + ct.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        out
    }

    pub fn decrypt(&self, blob: &[u8]) -> Result<Vec<u8>> {
        if blob.len() < 25 || blob[0] != VERSION {
            bail!("secreto cifrado con formato desconocido");
        }
        let nonce: [u8; 24] = blob[1..25].try_into().unwrap();
        self.cipher
            .decrypt(&XNonce::from(nonce), &blob[25..])
            .map_err(|_| anyhow::anyhow!("no se pudo descifrar (¿clave maestra distinta?)"))
    }

    pub fn encrypt_str(&self, s: &str) -> Vec<u8> {
        self.encrypt(s.as_bytes())
    }

    pub fn decrypt_str(&self, blob: &[u8]) -> Result<String> {
        Ok(String::from_utf8(self.decrypt(blob)?)?)
    }

    /// HMAC-SHA256 para buscar un secreto sin guardarlo en claro.
    pub fn lookup_hmac(&self, data: &str) -> Vec<u8> {
        let mut m = <SimpleHmac<Sha256> as KeyInit>::new_from_slice(&self.hmac_key).expect("hmac");
        m.update(data.as_bytes());
        m.finalize().into_bytes().to_vec()
    }
}

/// SHA-256 de un token de alta entropía (sesiones y tokens de API).
pub fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Token aleatorio de 32 bytes en hex (64 caracteres).
pub fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::fill(&mut b);
    hex::encode(b)
}

/// Salt para la autenticación por token de Subsonic.
pub fn random_salt() -> String {
    let mut b = [0u8; 8];
    rand::fill(&mut b);
    hex::encode(b)
}

pub fn subsonic_token(password: &str, salt: &str) -> String {
    use md5::Md5;
    let mut h = Md5::new();
    h.update(password.as_bytes());
    h.update(salt.as_bytes());
    hex::encode(h.finalize())
}

fn write_private(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path).with_context(|| format!("creando {}", path.display()))?;
    f.write_all(data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cifra_y_descifra() {
        let c = Crypto::from_key(&[7u8; 32]);
        let blob = c.encrypt_str("secreto");
        assert_ne!(&blob[25..], b"secreto");
        assert_eq!(c.decrypt_str(&blob).unwrap(), "secreto");
        // Dos cifrados del mismo texto no se parecen (nonce aleatorio).
        assert_ne!(blob, c.encrypt_str("secreto"));
    }

    #[test]
    fn clave_distinta_no_descifra() {
        let a = Crypto::from_key(&[1u8; 32]);
        let b = Crypto::from_key(&[2u8; 32]);
        assert!(b.decrypt(&a.encrypt_str("x")).is_err());
    }

    #[test]
    fn hmac_estable() {
        let c = Crypto::from_key(&[3u8; 32]);
        assert_eq!(c.lookup_hmac("abc"), c.lookup_hmac("abc"));
        assert_ne!(c.lookup_hmac("abc"), c.lookup_hmac("abd"));
    }

    #[test]
    fn token_subsonic_es_md5() {
        // md5("sesame" + "c19b2d") según la documentación de Subsonic.
        assert_eq!(subsonic_token("sesame", "c19b2d"), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn crea_y_recarga_clave() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("k");
        let a = Crypto::load_or_create(&p).unwrap();
        let b = Crypto::load_or_create(&p).unwrap();
        assert_eq!(b.decrypt_str(&a.encrypt_str("hola")).unwrap(), "hola");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
