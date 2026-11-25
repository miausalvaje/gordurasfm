//! Periodos de las estadísticas y sus cubos, calculados en la zona horaria del usuario.
//!
//! Los límites se calculan en hora local (lunes 00:00, día 1 00:00, 1 de enero 00:00) y luego se pasan
//! a UTC, así un día de 23 o 25 h por el cambio de hora es un único cubo correcto.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PeriodKey {
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LastYear,
    AllTime,
}

impl PeriodKey {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "this_week" => Self::ThisWeek,
            "last_week" => Self::LastWeek,
            "this_month" => Self::ThisMonth,
            "last_month" => Self::LastMonth,
            "this_year" => Self::ThisYear,
            "last_year" => Self::LastYear,
            "all_time" => Self::AllTime,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ThisWeek => "this_week",
            Self::LastWeek => "last_week",
            Self::ThisMonth => "this_month",
            Self::LastMonth => "last_month",
            Self::ThisYear => "this_year",
            Self::LastYear => "last_year",
            Self::AllTime => "all_time",
        }
    }

    /// Periodo en curso (todavía no ha terminado).
    pub fn is_current(self) -> bool {
        matches!(self, Self::ThisWeek | Self::ThisMonth | Self::ThisYear | Self::AllTime)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Day,
    Month,
    Year,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bucket {
    /// `2026-11-23` (día), `2026-11` (mes) o `2026` (año), en hora local.
    pub label: String,
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Period {
    pub key: PeriodKey,
    pub timezone: String,
    /// Inicio incluido y fin excluido (UTC, s). En los periodos en curso `to` es el final del periodo.
    pub from: i64,
    pub to: i64,
    pub unit: Unit,
    pub buckets: Vec<Bucket>,
    /// Periodo anterior para comparar (en los periodos en curso, recortado al mismo tiempo transcurrido).
    pub previous: Option<(i64, i64)>,
}

/// Medianoche local de `d`. Si no existe (zonas que cambian la hora a las 00:00), la primera hora válida.
pub fn local_midnight(tz: &Tz, d: NaiveDate) -> i64 {
    let mut naive = d.and_hms_opt(0, 0, 0).unwrap();
    for _ in 0..4 {
        if let Some(t) = tz.from_local_datetime(&naive).earliest() {
            return t.timestamp();
        }
        naive += Duration::hours(1);
    }
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).timestamp()
}

fn first_of_month(y: i32, m: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, 1).unwrap()
}

fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + n;
    first_of_month(total.div_euclid(12), total.rem_euclid(12) as u32 + 1)
}

fn day_buckets(tz: &Tz, from: NaiveDate, to: NaiveDate) -> Vec<Bucket> {
    let mut out = vec![];
    let mut d = from;
    while d < to {
        let n = d.succ_opt().unwrap();
        out.push(Bucket {
            label: d.format("%Y-%m-%d").to_string(),
            start: local_midnight(tz, d),
            end: local_midnight(tz, n),
        });
        d = n;
    }
    out
}

fn month_buckets(tz: &Tz, from: NaiveDate, to: NaiveDate) -> Vec<Bucket> {
    let mut out = vec![];
    let mut d = from;
    while d < to {
        let n = add_months(d, 1);
        out.push(Bucket {
            label: d.format("%Y-%m").to_string(),
            start: local_midnight(tz, d),
            end: local_midnight(tz, n),
        });
        d = n;
    }
    out
}

fn year_buckets(tz: &Tz, from: i32, to: i32) -> Vec<Bucket> {
    (from..to)
        .map(|y| Bucket {
            label: y.to_string(),
            start: local_midnight(tz, first_of_month(y, 1)),
            end: local_midnight(tz, first_of_month(y + 1, 1)),
        })
        .collect()
}

/// Calcula el periodo. `first_scrobble` (UTC, s) solo se usa en `all_time`.
pub fn resolve(key: PeriodKey, tz: Tz, now: DateTime<Utc>, first_scrobble: Option<i64>) -> Period {
    let today = now.with_timezone(&tz).date_naive();
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let month = first_of_month(today.year(), today.month());
    let year = first_of_month(today.year(), 1);
    let now_s = now.timestamp();

    let (from_d, to_d, unit) = match key {
        PeriodKey::ThisWeek => (monday, monday + Duration::days(7), Unit::Day),
        PeriodKey::LastWeek => (monday - Duration::days(7), monday, Unit::Day),
        PeriodKey::ThisMonth => (month, add_months(month, 1), Unit::Day),
        PeriodKey::LastMonth => (add_months(month, -1), month, Unit::Day),
        PeriodKey::ThisYear => (year, first_of_month(today.year() + 1, 1), Unit::Month),
        PeriodKey::LastYear => (first_of_month(today.year() - 1, 1), year, Unit::Month),
        PeriodKey::AllTime => {
            let first = first_scrobble
                .map(|t| DateTime::from_timestamp(t, 0).unwrap_or(now).with_timezone(&tz).date_naive())
                .unwrap_or(today)
                .min(today);
            let start = first_of_month(first.year(), first.month());
            let months = (today.year() - start.year()) * 12 + today.month() as i32 - start.month() as i32 + 1;
            if months <= 36 {
                (start, add_months(month, 1), Unit::Month)
            } else {
                (first_of_month(start.year(), 1), first_of_month(today.year() + 1, 1), Unit::Year)
            }
        }
    };

    let buckets = match unit {
        Unit::Day => day_buckets(&tz, from_d, to_d),
        Unit::Month => month_buckets(&tz, from_d, to_d),
        Unit::Year => year_buckets(&tz, from_d.year(), to_d.year()),
    };
    let from = local_midnight(&tz, from_d);
    let to = local_midnight(&tz, to_d);

    let previous = match key {
        PeriodKey::AllTime => None,
        _ => {
            let (pf, pt) = match key {
                PeriodKey::ThisWeek | PeriodKey::LastWeek => (from_d - Duration::days(7), from_d),
                PeriodKey::ThisMonth | PeriodKey::LastMonth => (add_months(from_d, -1), from_d),
                _ => (first_of_month(from_d.year() - 1, 1), from_d),
            };
            let (pf, pt) = (local_midnight(&tz, pf), local_midnight(&tz, pt));
            if key.is_current() {
                // Mismo tiempo transcurrido que el periodo actual.
                Some((pf, (pf + (now_s - from)).min(pt)))
            } else {
                Some((pf, pt))
            }
        }
    };

    Period { key, timezone: tz.name().to_string(), from, to, unit, buckets, previous }
}

/// Fecha local (`2026-11-23`) de un instante.
pub fn local_date(tz: &Tz, ts: i64) -> NaiveDate {
    DateTime::from_timestamp(ts, 0).unwrap_or_default().with_timezone(tz).date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn madrid() -> Tz {
        "Europe/Madrid".parse().unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn semana_empieza_el_lunes_en_hora_local() {
        // Miércoles 25-11-2026 23:30 en Madrid (22:30 UTC)
        let p = resolve(PeriodKey::ThisWeek, madrid(), at("2026-11-25T22:30:00Z"), None);
        assert_eq!(p.buckets.len(), 7);
        assert_eq!(p.buckets[0].label, "2026-11-23");
        assert_eq!(p.from, at("2026-11-22T23:00:00Z").timestamp()); // lunes 00:00 CET
        assert_eq!(p.to, at("2026-11-29T23:00:00Z").timestamp());
        // Lunes 30 a las 00:30 en Madrid ya es otra semana; el domingo a las 23:30, no.
        let p2 = resolve(PeriodKey::ThisWeek, madrid(), at("2026-11-29T23:30:00Z"), None);
        assert_eq!(p2.buckets[0].label, "2026-11-30", "lunes 00:30 ya es la semana siguiente");
        let p3 = resolve(PeriodKey::ThisWeek, madrid(), at("2026-11-29T22:30:00Z"), None);
        assert_eq!(p3.buckets[0].label, "2026-11-23");
    }

    #[test]
    fn cambio_de_hora_de_octubre_dia_de_25_horas() {
        let p = resolve(PeriodKey::ThisMonth, madrid(), at("2026-10-28T12:00:00Z"), None);
        assert_eq!(p.buckets.len(), 31);
        let d25 = p.buckets.iter().find(|b| b.label == "2026-10-25").unwrap();
        assert_eq!(d25.end - d25.start, 25 * 3600);
        let d24 = p.buckets.iter().find(|b| b.label == "2026-10-24").unwrap();
        assert_eq!(d24.end - d24.start, 24 * 3600);
        // Los cubos son contiguos
        for w in p.buckets.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
        assert_eq!(p.buckets[0].start, p.from);
        assert_eq!(p.buckets.last().unwrap().end, p.to);
    }

    #[test]
    fn cambio_de_hora_de_marzo_dia_de_23_horas() {
        let p = resolve(PeriodKey::LastMonth, madrid(), at("2026-04-10T12:00:00Z"), None);
        assert_eq!(p.buckets.len(), 31);
        let d29 = p.buckets.iter().find(|b| b.label == "2026-03-29").unwrap();
        assert_eq!(d29.end - d29.start, 23 * 3600);
    }

    #[test]
    fn mes_y_ano_pasados() {
        let now = at("2026-01-15T10:00:00Z");
        let lm = resolve(PeriodKey::LastMonth, madrid(), now, None);
        assert_eq!(lm.buckets.first().unwrap().label, "2025-12-01");
        assert_eq!(lm.buckets.len(), 31);
        let ly = resolve(PeriodKey::LastYear, madrid(), now, None);
        assert_eq!(ly.unit, Unit::Month);
        assert_eq!(ly.buckets.len(), 12);
        assert_eq!(ly.buckets[0].label, "2025-01");
        assert_eq!(ly.previous.unwrap().0, at("2023-12-31T23:00:00Z").timestamp());
    }

    #[test]
    fn todo_el_tiempo_por_meses_o_por_anos() {
        let now = at("2026-11-25T10:00:00Z");
        let corto = resolve(PeriodKey::AllTime, madrid(), now, Some(at("2025-02-10T10:00:00Z").timestamp()));
        assert_eq!(corto.unit, Unit::Month);
        assert_eq!(corto.buckets.first().unwrap().label, "2025-02");
        assert_eq!(corto.buckets.last().unwrap().label, "2026-11");
        let largo = resolve(PeriodKey::AllTime, madrid(), now, Some(at("2012-06-01T10:00:00Z").timestamp()));
        assert_eq!(largo.unit, Unit::Year);
        assert_eq!(largo.buckets.len(), 15);
        assert_eq!(largo.buckets[0].label, "2012");
        assert!(largo.previous.is_none());
        // Sin escuchas
        let vacio = resolve(PeriodKey::AllTime, madrid(), now, None);
        assert_eq!(vacio.buckets.len(), 1);
    }

    #[test]
    fn comparacion_recortada_en_periodos_en_curso() {
        let now = at("2026-11-25T11:00:00Z"); // miércoles 12:00 en Madrid
        let p = resolve(PeriodKey::ThisWeek, madrid(), now, None);
        let (pf, pt) = p.previous.unwrap();
        assert_eq!(pf, p.from - 7 * 86400);
        assert_eq!(pt - pf, now.timestamp() - p.from);
    }

    #[test]
    fn otra_zona_horaria() {
        let mx: Tz = "America/Mexico_City".parse().unwrap();
        // 25-11 02:00 UTC = 24-11 20:00 en México
        let p = resolve(PeriodKey::ThisMonth, mx, at("2026-12-01T02:00:00Z"), None);
        assert_eq!(p.buckets[0].label, "2026-11-01", "en México todavía es noviembre");
    }
}
