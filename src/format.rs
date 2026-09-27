//! The `format` values this contract asserts.
//!
//! A format is asserted, not annotated: a Location that names one means it.
//! A format this file does not know holds, as the specification says an
//! unknown format must, so it is not compiled at all. Dates and times are
//! RFC 3339's, read by the estate's one calendar (`codec::civil`), so the
//! thirty-first of February is no date.

use codec::civil;
use regex::Regex;

/// A format this contract checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// RFC 3339 `full-date`.
    Date,
    /// RFC 3339 `full-time`: a time of day and its offset.
    Time,
    /// RFC 3339 `date-time`.
    DateTime,
    Email,
    Uri,
    Ipv4,
    Ipv6,
    Uuid,
    Regex,
}

impl Format {
    /// The format `name` names, when it is one this contract checks.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "date" => Self::Date,
            "time" => Self::Time,
            "date-time" => Self::DateTime,
            "email" => Self::Email,
            "uri" => Self::Uri,
            "ipv4" => Self::Ipv4,
            "ipv6" => Self::Ipv6,
            "uuid" => Self::Uuid,
            "regex" => Self::Regex,
            _ => return None,
        })
    }

    /// Whether `text` is of this format.
    #[must_use]
    pub fn holds(self, text: &str) -> bool {
        match self {
            Self::Date => civil::read_date(text).is_some(),
            Self::Time => civil::read_time(text)
                .and_then(|(_, zone)| civil::read_offset(zone))
                .is_some(),
            Self::DateTime => civil::read_rfc3339(text).is_some(),
            Self::Email => text.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
            }),
            Self::Uri => text.split_once(':').is_some_and(|(scheme, rest)| {
                !rest.is_empty()
                    && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
            }),
            Self::Ipv4 => text.parse::<std::net::Ipv4Addr>().is_ok(),
            Self::Ipv6 => text.parse::<std::net::Ipv6Addr>().is_ok(),
            Self::Uuid => {
                text.split('-').map(str::len).eq([8, 4, 4, 4, 12])
                    && text.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
            }
            // The instance is itself a pattern: compiling it is the check.
            Self::Regex => Regex::new(text).is_ok(),
        }
    }
}
