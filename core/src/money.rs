//! Money over time (purchases spec §3.9): a purchase price in today's money. One country per
//! inventory: an amount in another currency is turned into the home currency at the purchase
//! day's rate, then grown by the home price index from the purchase month to the latest one.
//! The index and the rates are cached here; `tools/money` fetches them, so `ev` stays offline.

use chrono::{Datelike, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::purchases::money;
use crate::store::{Inventory, now};
use crate::{Error, Result};

/// A cached index older than this (from the first day of its latest month) is reported stale.
const STALE_DAYS: i64 = 75;

/// How far back a missing day's rate may be taken from (a weekend, a holiday).
const RATE_LOOKBACK_DAYS: i64 = 7;

fn setting(conn: &Connection, key: &str, default: &str) -> Result<String> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?
        .unwrap_or_else(|| default.to_string()))
}

pub(crate) fn home_currency(conn: &Connection) -> Result<String> {
    setting(conn, "home_currency", "TRY")
}

pub(crate) fn home_country(conn: &Connection) -> Result<String> {
    setting(conn, "home_country", "TR")
}

fn series(conn: &Connection) -> Result<String> {
    setting(conn, "price_index", "eurostat:TR")
}

fn day(d: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(d.get(..10).unwrap_or(d), "%Y-%m-%d").ok()
}

/// The index for a month: the month's own value, else its year's (an annual series).
fn index_at(conn: &Connection, series: &str, month: &str) -> Result<Option<f64>> {
    let v: Option<f64> = conn
        .query_row(
            "SELECT value FROM price_index WHERE series = ?1 AND period = ?2",
            params![series, month],
            |r| r.get(0),
        )
        .optional()?;
    if v.is_some() {
        return Ok(v);
    }
    Ok(conn
        .query_row(
            "SELECT value FROM price_index WHERE series = ?1 AND period = ?2",
            params![series, &month[..4]],
            |r| r.get(0),
        )
        .optional()?)
}

/// The latest period of the series and its value.
fn latest(conn: &Connection, series: &str) -> Result<Option<(String, f64)>> {
    Ok(conn
        .query_row(
            "SELECT period, value FROM price_index WHERE series = ?1
              ORDER BY length(period) DESC, period DESC LIMIT 1",
            [series],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

/// Home currency per unit of `currency` on `d`, or the closest earlier day within a week.
fn rate_at(
    conn: &Connection,
    currency: &str,
    home: &str,
    d: NaiveDate,
) -> Result<Option<(f64, String)>> {
    let from = d - chrono::Days::new(RATE_LOOKBACK_DAYS as u64);
    Ok(conn
        .query_row(
            "SELECT rate, day FROM fx_rates WHERE currency = ?1 AND home = ?2
               AND day <= ?3 AND day >= ?4 ORDER BY day DESC LIMIT 1",
            params![currency, home, d.to_string(), from.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

/// `cents` of `currency` paid on `date`, in today's home money: `{amount, currency, index_month,
/// rate, rate_day}`, or None when the index or a needed rate is not cached.
pub(crate) fn today_money(
    conn: &Connection,
    cents: i64,
    currency: Option<&str>,
    date: Option<&str>,
) -> Result<Option<Value>> {
    let home = home_currency(conn)?;
    let currency = currency.unwrap_or(&home).to_uppercase();
    let Some(d) = date.and_then(day) else {
        return Ok(None);
    };
    let series = series(conn)?;
    let Some((latest_period, latest_value)) = latest(conn, &series)? else {
        return Ok(None);
    };
    let month = format!("{:04}-{:02}", d.year(), d.month());
    let Some(then) = index_at(conn, &series, &month)? else {
        return Ok(None);
    };
    let (in_home, rate) = if currency == home {
        (cents as f64, None)
    } else {
        match rate_at(conn, &currency, &home, d)? {
            Some((r, on)) => (cents as f64 * r, Some((r, on))),
            None => return Ok(None),
        }
    };
    let today = (in_home * latest_value / then).round() as i64;
    let mut v = json!({
        "amount": money(today),
        "currency": home,
        "index": series,
        "index_month": latest_period,
    });
    if let Some((r, on)) = rate {
        v["rate"] = json!(r);
        v["rate_day"] = json!(on);
        v["in_home_then"] = json!(money(in_home.round() as i64));
    }
    Ok(Some(v))
}

impl Inventory {
    /// What `tools/money` should fetch: the index series from the earliest purchase month, and
    /// the rate of every foreign-currency purchase day not cached.
    pub fn money_needs(&self) -> Result<Value> {
        let home = home_currency(&self.conn)?;
        let series = series(&self.conn)?;
        let from: Option<String> = self.conn.query_row(
            "SELECT MIN(substr(COALESCE(delivered_at, ordered_at), 1, 7)) FROM purchases
              WHERE COALESCE(delivered_at, ordered_at) IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT UPPER(currency), substr(COALESCE(delivered_at, ordered_at), 1, 10)
               FROM purchases
              WHERE currency IS NOT NULL AND UPPER(currency) != ?1
                AND COALESCE(delivered_at, ordered_at) IS NOT NULL
              ORDER BY 1, 2",
        )?;
        let wanted = stmt
            .query_map([&home], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut rates = Vec::new();
        for (c, d) in wanted {
            if let Some(dd) = day(&d)
                && rate_at(&self.conn, &c, &home, dd)?.is_none()
            {
                rates.push(json!({ "currency": c, "day": d }));
            }
        }
        Ok(json!({
            "money_needs": {
                "home_currency": home,
                "home_country": setting(&self.conn, "home_country", "TR")?,
                "index": series,
                "from_month": from,
                "rates": rates,
            }
        }))
    }

    /// Imports index and rate lines from `tools/money` (`{"type":"index","series","period",
    /// "value","source"}`, `{"type":"rate","currency","home","day","rate","source"}`). All or
    /// nothing; a value fetched again replaces the cached one.
    pub fn money_import(&mut self, ndjson: &str) -> Result<Value> {
        let tx = self.conn.transaction()?;
        let (mut indexes, mut rates) = (0, 0);
        for (i, raw) in ndjson.lines().enumerate() {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let at = |e: Error| e.at_line(i + 1);
            let v: Value = serde_json::from_str(raw)
                .map_err(|e| Error::Usage(format!("not JSON: {e}")))
                .map_err(at)?;
            let s = |k: &str| v[k].as_str().map(str::to_string);
            let need = |k: &str| {
                s(k).ok_or_else(|| Error::Usage(format!("`{k}` is required")).at_line(i + 1))
            };
            let num = |k: &str| {
                v[k].as_f64()
                    .filter(|x| x.is_finite() && *x > 0.0)
                    .ok_or_else(|| {
                        Error::Usage(format!("`{k}` must be a positive number")).at_line(i + 1)
                    })
            };
            match v["type"].as_str() {
                Some("index") => {
                    let period = need("period")?;
                    let ok = period.len() == 4 && period.chars().all(|c| c.is_ascii_digit())
                        || period.len() == 7 && day(&format!("{period}-01")).is_some();
                    if !ok {
                        return Err(Error::Usage(format!("period `{period}`: YYYY-MM or YYYY"))
                            .at_line(i + 1));
                    }
                    tx.execute(
                        "INSERT INTO price_index (series, period, value, source, fetched_at)
                         VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT (series, period) DO UPDATE SET value = excluded.value,
                           source = excluded.source, fetched_at = excluded.fetched_at",
                        params![need("series")?, period, num("value")?, s("source"), now()],
                    )?;
                    indexes += 1;
                }
                Some("rate") => {
                    let d = need("day")?;
                    if day(&d).is_none() {
                        return Err(Error::Usage(format!("day `{d}`: YYYY-MM-DD")).at_line(i + 1));
                    }
                    tx.execute(
                        "INSERT INTO fx_rates (currency, home, day, rate, source, fetched_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                         ON CONFLICT (currency, home, day) DO UPDATE SET rate = excluded.rate,
                           source = excluded.source, fetched_at = excluded.fetched_at",
                        params![
                            need("currency")?.to_uppercase(),
                            need("home")?.to_uppercase(),
                            &d[..10],
                            num("rate")?,
                            s("source"),
                            now()
                        ],
                    )?;
                    rates += 1;
                }
                other => {
                    return Err(
                        Error::Usage(format!("line type {other:?}; use index or rate"))
                            .at_line(i + 1),
                    );
                }
            }
        }
        tx.commit()?;
        Ok(json!({ "money_imported": { "index": indexes, "rates": rates } }))
    }

    /// What is cached: the series, its latest period and whether it is stale, how many rates.
    pub fn money_status(&self) -> Result<Value> {
        let series = series(&self.conn)?;
        let latest = latest(&self.conn, &series)?;
        let periods: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM price_index WHERE series = ?1",
            [&series],
            |r| r.get(0),
        )?;
        let rates: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM fx_rates", [], |r| r.get(0))?;
        let stale = latest.as_ref().is_none_or(|(p, _)| {
            let first = if p.len() == 7 {
                day(&format!("{p}-01"))
            } else {
                day(&format!("{p}-12-01"))
            };
            first.is_none_or(|f| (chrono::Utc::now().date_naive() - f).num_days() > STALE_DAYS)
        });
        Ok(json!({
            "money": {
                "home_currency": home_currency(&self.conn)?,
                "index": series,
                "periods": periods,
                "latest": latest.map(|(p, _)| p),
                "stale": stale,
                "rates": rates,
                "missing_rates": self.money_needs()?["money_needs"]["rates"].as_array().map_or(0, Vec::len),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::day;

    #[test]
    fn a_day_is_read_from_a_date_or_a_timestamp() {
        assert_eq!(day("2024-05-03").unwrap().to_string(), "2024-05-03");
        assert_eq!(
            day("2024-05-03T10:00:00Z").unwrap().to_string(),
            "2024-05-03"
        );
        assert!(day("03.05.2024").is_none());
    }
}
