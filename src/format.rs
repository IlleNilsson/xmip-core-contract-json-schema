//! The `pattern` and `format` checks of the string vocabulary.
//!
//! A pattern compiles once per process and is an unanchored search, as JSON
//! Schema says. A format is asserted, not annotated: a Location that names one
//! means it. Formats this file does not know hold, as the specification says
//! an unknown format must.

use regex::Regex;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A pattern compiles once per process. JSON Schema patterns are unanchored
/// searches, so the text is used as written.
pub(crate) fn compiled(pattern: &str) -> Option<Regex> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    cache
        .entry(pattern.to_string())
        .or_insert_with(|| Regex::new(pattern).ok())
        .clone()
}

pub(crate) fn holds_format(format: &str, text: &str) -> bool {
    match format {
        "date" => is_date(text),
        "time" => is_time(text),
        "date-time" => text
            .split_once(['T', 't'])
            .is_some_and(|(date, time)| is_date(date) && is_time(time)),
        "email" => text.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        }),
        "uri" => text.split_once(':').is_some_and(|(scheme, rest)| {
            !rest.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        }),
        "ipv4" => text.parse::<std::net::Ipv4Addr>().is_ok(),
        "ipv6" => text.parse::<std::net::Ipv6Addr>().is_ok(),
        "uuid" => {
            let parts: Vec<&str> = text.split('-').collect();
            parts.iter().map(|p| p.len()).eq([8, 4, 4, 4, 12])
                && text.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
        }
        "regex" => Regex::new(text).is_ok(),
        _ => true,
    }
}

fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
        && (1..=12).contains(&parts[1].parse::<u8>().unwrap_or(0))
        && (1..=31).contains(&parts[2].parse::<u8>().unwrap_or(0))
}

fn is_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 8
        && bytes[..8].iter().enumerate().all(|(i, b)| {
            if i == 2 || i == 5 {
                *b == b':'
            } else {
                b.is_ascii_digit()
            }
        })
        && text[8..]
            .chars()
            .all(|c| c.is_ascii_digit() || "Zz+-:.".contains(c))
}
