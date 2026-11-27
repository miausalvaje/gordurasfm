//! Límite de intentos de inicio de sesión.
//!
//! Cada intento le cuesta a OwnFM una consulta a Navidrome, así que sin límite sería un oráculo para
//! adivinar contraseñas de Navidrome por fuerza bruta. Se cuentan los intentos (no solo los fallos, para
//! que no se cuelen muchas peticiones a la vez) por IP de origen y por usuario dentro de una ventana; un
//! inicio de sesión correcto pone a cero los dos contadores.
//!
//! La IP de origen se saca de `X-Forwarded-For` solo si la conexión llega desde un proxy de confianza
//! (`trusted_proxies`); si no, se usa la IP de la conexión.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::http::HeaderMap;

/// Intentos permitidos por clave (IP o usuario) dentro de la ventana.
pub const MAX_ATTEMPTS: u32 = 10;
/// Duración de la ventana.
pub const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Tope de claves en memoria: por encima, se rechaza (mejor eso que quedarse sin memoria).
const MAX_KEYS: usize = 100_000;

struct Entry {
    count: u32,
    since: Instant,
}

pub struct LoginLimiter {
    max: u32,
    window: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new(MAX_ATTEMPTS, WINDOW)
    }
}

impl LoginLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self { max, window, entries: Mutex::new(HashMap::new()) }
    }

    /// Claves de un intento: la IP (si se conoce) y el usuario en minúsculas.
    pub fn keys(ip: Option<IpAddr>, username: &str) -> Vec<String> {
        let mut k = Vec::with_capacity(2);
        if let Some(ip) = ip {
            k.push(format!("ip:{ip}"));
        }
        k.push(format!("u:{}", username.to_lowercase().chars().take(100).collect::<String>()));
        k
    }

    /// Apunta un intento. `Err(segundos)` si alguna clave ha agotado sus intentos (y entonces no apunta nada).
    pub fn attempt(&self, keys: &[String]) -> Result<(), u64> {
        self.attempt_at(keys, Instant::now())
    }

    pub fn attempt_at(&self, keys: &[String], now: Instant) -> Result<(), u64> {
        let mut m = self.entries.lock().unwrap();
        if m.len() >= MAX_KEYS / 2 {
            m.retain(|_, e| now.duration_since(e.since) < self.window);
        }
        let mut wait = 0u64;
        for k in keys {
            if let Some(e) = m.get(k)
                && now.duration_since(e.since) < self.window
                && e.count >= self.max
            {
                let left = self.window - now.duration_since(e.since);
                wait = wait.max(left.as_secs().max(1));
            }
        }
        if wait > 0 {
            return Err(wait);
        }
        if m.len() + keys.len() > MAX_KEYS {
            return Err(self.window.as_secs());
        }
        for k in keys {
            let e = m.entry(k.clone()).or_insert(Entry { count: 0, since: now });
            if now.duration_since(e.since) >= self.window {
                *e = Entry { count: 0, since: now };
            }
            e.count += 1;
        }
        Ok(())
    }

    /// Inicio de sesión correcto: borra los contadores de esas claves.
    pub fn success(&self, keys: &[String]) {
        let mut m = self.entries.lock().unwrap();
        for k in keys {
            m.remove(k);
        }
    }
}

/// Red en notación CIDR (`192.168.1.0/24`, `fd00::/8`) o una IP suelta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpNet {
    addr: IpAddr,
    prefix: u8,
}

impl IpNet {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (ip, prefix) = match s.split_once('/') {
            Some((ip, p)) => (ip, Some(p)),
            None => (s, None),
        };
        let addr: IpAddr = ip.parse().map_err(|_| format!("IP no válida: {s}"))?;
        let addr = addr.to_canonical();
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p.parse::<u8>().ok().filter(|p| *p <= max).ok_or_else(|| format!("prefijo no válido: {s}"))?,
            None => max,
        };
        Ok(Self { addr, prefix })
    }

    pub fn contains(&self, ip: &IpAddr) -> bool {
        match (self.addr, ip.to_canonical()) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if self.prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(self.prefix)) };
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if self.prefix == 0 { 0 } else { u128::MAX << (128 - u32::from(self.prefix)) };
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

/// IP del cliente: la de la conexión o, si viene de un proxy de confianza, la última de `X-Forwarded-For`
/// que no sea de un proxy de confianza (las de la izquierda las puede inventar el cliente).
pub fn client_ip(peer: Option<IpAddr>, headers: &HeaderMap, trusted: &[IpNet]) -> Option<IpAddr> {
    let peer = peer?.to_canonical();
    let is_trusted = |ip: &IpAddr| trusted.iter().any(|n| n.contains(ip));
    if !is_trusted(&peer) {
        return Some(peer);
    }
    let hops: Vec<IpAddr> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .map(|ip| ip.to_canonical())
        .collect();
    Some(hops.into_iter().rev().find(|ip| !is_trusted(ip)).unwrap_or(peer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn bloquea_al_agotar_los_intentos_y_se_libera_al_pasar_la_ventana() {
        let l = LoginLimiter::new(3, Duration::from_secs(60));
        let t0 = Instant::now();
        let k = LoginLimiter::keys(Some(ip("1.2.3.4")), "Ana");
        for _ in 0..3 {
            assert!(l.attempt_at(&k, t0).is_ok());
        }
        let wait = l.attempt_at(&k, t0 + Duration::from_secs(10)).unwrap_err();
        assert_eq!(wait, 50);
        // El mismo usuario desde otra IP también está bloqueado (clave de usuario, sin distinguir mayúsculas).
        assert!(l.attempt_at(&LoginLimiter::keys(Some(ip("5.6.7.8")), "ana"), t0).is_err());
        // Otro usuario desde otra IP, no.
        assert!(l.attempt_at(&LoginLimiter::keys(Some(ip("5.6.7.8")), "bea"), t0).is_ok());
        // Pasada la ventana, vuelve a dejar.
        assert!(l.attempt_at(&k, t0 + Duration::from_secs(61)).is_ok());
    }

    #[test]
    fn un_acierto_pone_a_cero() {
        let l = LoginLimiter::new(2, Duration::from_secs(60));
        let t0 = Instant::now();
        let k = LoginLimiter::keys(Some(ip("1.2.3.4")), "ana");
        l.attempt_at(&k, t0).unwrap();
        l.attempt_at(&k, t0).unwrap();
        assert!(l.attempt_at(&k, t0).is_err());
        l.success(&k);
        assert!(l.attempt_at(&k, t0).is_ok());
    }

    #[test]
    fn redes() {
        let n = IpNet::parse("192.168.1.0/24").unwrap();
        assert!(n.contains(&ip("192.168.1.248")));
        assert!(!n.contains(&ip("192.168.2.1")));
        assert!(n.contains(&ip("::ffff:192.168.1.7")), "IPv4 mapeada en IPv6");
        assert!(IpNet::parse("10.0.0.1").unwrap().contains(&ip("10.0.0.1")));
        assert!(!IpNet::parse("10.0.0.1").unwrap().contains(&ip("10.0.0.2")));
        assert!(IpNet::parse("fd00::/8").unwrap().contains(&ip("fd12::1")));
        assert!(IpNet::parse("0.0.0.0/0").unwrap().contains(&ip("8.8.8.8")));
        assert!(IpNet::parse("1.2.3.4/33").is_err());
        assert!(IpNet::parse("nada").is_err());
    }

    #[test]
    fn ip_del_cliente() {
        let trusted = [IpNet::parse("192.168.1.248").unwrap()];
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "6.6.6.6, 203.0.113.9".parse().unwrap());
        // Desde el proxy: la última que no es del proxy (la primera la puede inventar el cliente).
        assert_eq!(client_ip(Some(ip("192.168.1.248")), &h, &trusted), Some(ip("203.0.113.9")));
        // Directo (no es un proxy de confianza): se ignora la cabecera.
        assert_eq!(client_ip(Some(ip("203.0.113.50")), &h, &trusted), Some(ip("203.0.113.50")));
        // Sin proxies de confianza configurados, tampoco se mira.
        assert_eq!(client_ip(Some(ip("192.168.1.248")), &h, &[]), Some(ip("192.168.1.248")));
        // Desde el proxy sin cabecera: la del proxy.
        assert_eq!(client_ip(Some(ip("192.168.1.248")), &HeaderMap::new(), &trusted), Some(ip("192.168.1.248")));
        assert_eq!(client_ip(None, &h, &trusted), None);
    }
}
