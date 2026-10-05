//! Instant answers: arithmetic, percentages, unit conversion and currency
//! conversion. Everything except currency works offline; exchange rates are
//! fetched at most twice a day, only when a currency question is typed, and
//! are cached on disk so they keep working without a connection.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// An answer shown above the results.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Answer {
    /// The result, e.g. "3.1069 mi".
    pub value: String,
    /// "calculator", "unit" or "currency".
    pub kind: String,
    /// What was asked, normalised, e.g. "5 km"; empty for plain arithmetic.
    pub detail: String,
}

// ────────────────────── Number formatting ──────────────────────

/// Format a result without noise: whole numbers plainly, others with just
/// enough decimals.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let decimals = if value.abs() >= 1.0 { 4 } else { 8 };
    let text = format!("{:.*}", decimals, value);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Money: two decimals and thousands separators.
fn format_money(value: f64) -> String {
    let text = format!("{:.2}", value.abs());
    let (whole, cents) = text.split_once('.').unwrap_or((&text, "00"));
    let mut grouped = String::new();
    for (i, digit) in whole.chars().enumerate() {
        if i > 0 && (whole.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{}{}.{}", if value < 0.0 { "-" } else { "" }, grouped, cents)
}

// ────────────────────── Arithmetic ──────────────────────

/// Evaluate an arithmetic expression: + - * / % ^ and parentheses.
fn arithmetic(query: &str) -> Option<f64> {
    let trimmed = query.trim();

    // Must contain at least one operator and one digit
    if !trimmed.chars().any(|c| c.is_ascii_digit())
        || !trimmed.chars().any(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '^'))
    {
        return None;
    }

    // Only allow safe characters
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '*' | '/' | '.' | '(' | ')' | ' ' | '%' | '^'))
    {
        return None;
    }

    match parse_expression(trimmed) {
        Some((result, rest)) if rest.trim().is_empty() && result.is_finite() => Some(result),
        _ => None,
    }
}

fn parse_expression(input: &str) -> Option<(f64, &str)> {
    let (mut left, mut rest) = parse_term(input)?;
    let mut rest_trimmed = rest.trim_start();
    while rest_trimmed.starts_with('+') || rest_trimmed.starts_with('-') {
        let op = rest_trimmed.chars().next()?;
        let after_op = rest_trimmed[1..].trim_start();
        let (right, new_rest) = parse_term(after_op)?;
        left = match op {
            '+' => left + right,
            '-' => left - right,
            _ => unreachable!(),
        };
        rest = new_rest;
        rest_trimmed = rest.trim_start();
    }
    Some((left, rest))
}

fn parse_term(input: &str) -> Option<(f64, &str)> {
    let (mut left, mut rest) = parse_power(input)?;
    let mut rest_trimmed = rest.trim_start();
    while rest_trimmed.starts_with('*') || rest_trimmed.starts_with('/') || rest_trimmed.starts_with('%') {
        let op = rest_trimmed.chars().next()?;
        let after_op = rest_trimmed[1..].trim_start();
        let (right, new_rest) = parse_power(after_op)?;
        left = match op {
            '*' => left * right,
            '/' => {
                if right == 0.0 {
                    return None; // division by zero
                }
                left / right
            }
            '%' => {
                if right == 0.0 {
                    return None;
                }
                left % right
            }
            _ => unreachable!(),
        };
        rest = new_rest;
        rest_trimmed = rest.trim_start();
    }
    Some((left, rest))
}

/// Exponentiation binds tighter than multiplication and is right-associative.
fn parse_power(input: &str) -> Option<(f64, &str)> {
    let (base, rest) = parse_factor(input)?;
    let rest_trimmed = rest.trim_start();
    if let Some(after) = rest_trimmed.strip_prefix('^') {
        let (exponent, new_rest) = parse_power(after)?;
        return Some((base.powf(exponent), new_rest));
    }
    Some((base, rest))
}

fn parse_factor(input: &str) -> Option<(f64, &str)> {
    let trimmed = input.trim_start();

    // Handle parentheses
    if trimmed.starts_with('(') {
        let (val, rest) = parse_expression(&trimmed[1..])?;
        let rest = rest.trim_start();
        if rest.starts_with(')') {
            return Some((val, &rest[1..]));
        }
        return None;
    }

    // Handle negative numbers
    if trimmed.starts_with('-') {
        let (val, rest) = parse_factor(&trimmed[1..])?;
        return Some((-val, rest));
    }

    let (num, rest) = leading_number(trimmed)?;
    Some((num, rest))
}

/// Split "12.5 km" into (12.5, " km").
fn leading_number(input: &str) -> Option<(f64, &str)> {
    let mut end = 0;
    let mut has_dot = false;
    for (i, c) in input.char_indices() {
        if c.is_ascii_digit() || (c == ',' && end > 0) {
            end = i + 1;
        } else if c == '.' && !has_dot {
            has_dot = true;
            end = i + 1;
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let num: f64 = input[..end].replace(',', "").parse().ok()?;
    Some((num, &input[end..]))
}

/// "15% of 240" → 36
fn percent_of(query: &str) -> Option<Answer> {
    let (left, right) = query.split_once(" of ")?;
    let percent = arithmetic_or_number(left.trim().strip_suffix('%')?)?;
    let whole = arithmetic_or_number(right)?;
    Some(Answer {
        value: format_number(whole * percent / 100.0),
        kind: "calculator".to_string(),
        detail: format!("{}% of {}", format_number(percent), format_number(whole)),
    })
}

fn arithmetic_or_number(text: &str) -> Option<f64> {
    let text = text.trim();
    arithmetic(text).or_else(|| match leading_number(text) {
        Some((n, rest)) if rest.is_empty() => Some(n),
        _ => None,
    })
}

// ────────────────────── Units ──────────────────────

/// (dimension, symbol shown in answers, size in the dimension's base unit, names accepted)
type Unit = (&'static str, &'static str, f64, &'static [&'static str]);

const UNITS: &[Unit] = &[
    // Length, in metres
    ("length", "mm", 0.001, &["mm", "millimeter", "millimeters", "millimetre", "millimetres"]),
    ("length", "cm", 0.01, &["cm", "centimeter", "centimeters", "centimetre", "centimetres"]),
    ("length", "m", 1.0, &["m", "meter", "meters", "metre", "metres"]),
    ("length", "km", 1000.0, &["km", "kilometer", "kilometers", "kilometre", "kilometres"]),
    ("length", "in", 0.0254, &["in", "inch", "inches"]),
    ("length", "ft", 0.3048, &["ft", "foot", "feet"]),
    ("length", "yd", 0.9144, &["yd", "yard", "yards"]),
    ("length", "mi", 1609.344, &["mi", "mile", "miles"]),
    // Mass, in kilograms
    ("mass", "mg", 1e-6, &["mg", "milligram", "milligrams"]),
    ("mass", "g", 0.001, &["g", "gram", "grams"]),
    ("mass", "kg", 1.0, &["kg", "kilo", "kilos", "kilogram", "kilograms"]),
    ("mass", "t", 1000.0, &["t", "tonne", "tonnes"]),
    ("mass", "oz", 0.028349523125, &["oz", "ounce", "ounces"]),
    ("mass", "lb", 0.45359237, &["lb", "lbs", "pound", "pounds"]),
    ("mass", "st", 6.35029318, &["st", "stone", "stones"]),
    // Volume, in litres
    ("volume", "ml", 0.001, &["ml", "milliliter", "milliliters", "millilitre", "millilitres"]),
    ("volume", "l", 1.0, &["l", "liter", "liters", "litre", "litres"]),
    ("volume", "gal", 3.785411784, &["gal", "gallon", "gallons"]),
    ("volume", "pt", 0.473176473, &["pt", "pint", "pints"]),
    ("volume", "cup", 0.2365882365, &["cup", "cups"]),
    ("volume", "tbsp", 0.01478676478, &["tbsp", "tablespoon", "tablespoons"]),
    ("volume", "tsp", 0.00492892159, &["tsp", "teaspoon", "teaspoons"]),
    // Area, in square metres
    ("area", "m²", 1.0, &["m2", "sqm", "m²"]),
    ("area", "ft²", 0.09290304, &["ft2", "sqft", "ft²"]),
    ("area", "km²", 1e6, &["km2", "sqkm", "km²"]),
    ("area", "acre", 4046.8564224, &["acre", "acres"]),
    ("area", "ha", 10000.0, &["ha", "hectare", "hectares"]),
    // Speed, in metres per second
    ("speed", "m/s", 1.0, &["m/s", "mps"]),
    ("speed", "km/h", 0.2777777778, &["km/h", "kmh", "kph"]),
    ("speed", "mph", 0.44704, &["mph"]),
    ("speed", "kn", 0.5144444444, &["kn", "knot", "knots"]),
    // Time, in seconds
    ("time", "ms", 0.001, &["ms", "millisecond", "milliseconds"]),
    ("time", "s", 1.0, &["s", "sec", "secs", "second", "seconds"]),
    ("time", "min", 60.0, &["min", "mins", "minute", "minutes"]),
    ("time", "h", 3600.0, &["h", "hr", "hrs", "hour", "hours"]),
    ("time", "days", 86400.0, &["d", "day", "days"]),
    ("time", "weeks", 604800.0, &["wk", "week", "weeks"]),
    ("time", "years", 31557600.0, &["yr", "year", "years"]),
    // Data, in bytes
    ("data", "bytes", 1.0, &["byte", "bytes"]),
    ("data", "KB", 1e3, &["kb"]),
    ("data", "MB", 1e6, &["mb"]),
    ("data", "GB", 1e9, &["gb"]),
    ("data", "TB", 1e12, &["tb"]),
    ("data", "KiB", 1024.0, &["kib"]),
    ("data", "MiB", 1048576.0, &["mib"]),
    ("data", "GiB", 1073741824.0, &["gib"]),
];

fn find_unit(name: &str) -> Option<&'static Unit> {
    UNITS.iter().find(|unit| unit.3.contains(&name))
}

/// Temperature scales are offsets, not factors: (symbol, to Celsius, from Celsius).
fn temperature(name: &str) -> Option<(&'static str, fn(f64) -> f64, fn(f64) -> f64)> {
    match name {
        "c" | "°c" | "celsius" => Some(("°C", |v| v, |c| c)),
        "f" | "°f" | "fahrenheit" => Some(("°F", |v| (v - 32.0) * 5.0 / 9.0, |c| c * 9.0 / 5.0 + 32.0)),
        "k" | "kelvin" => Some(("K", |v| v - 273.15, |c| c + 273.15)),
        _ => None,
    }
}

// ────────────────────── Currency ──────────────────────

/// Everyday names and symbols for the currencies people type most.
fn currency_alias(name: &str) -> Option<&'static str> {
    Some(match name {
        "$" | "dollar" | "dollars" => "USD",
        "₹" | "rs" | "rupee" | "rupees" => "INR",
        "€" | "euro" | "euros" => "EUR",
        "£" | "quid" => "GBP",
        "¥" | "yen" => "JPY",
        "yuan" => "CNY",
        _ => return None,
    })
}

/// Whether `name` could be a currency: a known alias, or a three-letter code
/// that is not a unit.
fn currency_code(name: &str) -> Option<String> {
    if let Some(code) = currency_alias(name) {
        return Some(code.to_string());
    }
    let looks_like_code = name.len() == 3 && name.chars().all(|c| c.is_ascii_alphabetic());
    (looks_like_code && find_unit(name).is_none() && temperature(name).is_none()).then(|| name.to_uppercase())
}

/// Exchange rates against the US dollar.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateTable {
    /// Unix time the rates were published.
    pub updated: i64,
    /// Unix time we downloaded them.
    pub fetched_at: i64,
    pub rates: HashMap<String, f64>,
}

const RATES_URL: &str = "https://open.er-api.com/v6/latest/USD";
const RATES_MAX_AGE_SECS: i64 = 12 * 60 * 60;

/// Exchange rates, cached in memory and on disk.
pub struct Rates {
    path: PathBuf,
    table: Mutex<Option<RateTable>>,
    refreshing: AtomicBool,
}

impl Rates {
    pub fn load(path: PathBuf) -> Self {
        let table = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        Rates { path, table: Mutex::new(table), refreshing: AtomicBool::new(false) }
    }

    pub fn table(&self) -> Option<RateTable> {
        self.table.lock().unwrap().clone()
    }

    /// Download new rates if the cached ones are missing or old. Failures are
    /// silent: the previous rates simply stay in use.
    pub async fn refresh_if_stale(&self) {
        let now = chrono::Utc::now().timestamp();
        let fresh = self.table().is_some_and(|t| now - t.fetched_at < RATES_MAX_AGE_SECS);
        if fresh || self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        match fetch_rates(now).await {
            Ok(table) => {
                if let Ok(text) = serde_json::to_string(&table) {
                    let _ = std::fs::write(&self.path, text);
                }
                *self.table.lock().unwrap() = Some(table);
            }
            Err(e) => log::warn!("Could not update exchange rates: {}", e),
        }
        self.refreshing.store(false, Ordering::SeqCst);
    }
}

#[derive(Deserialize)]
struct RatesResponse {
    result: String,
    time_last_update_unix: i64,
    rates: HashMap<String, f64>,
}

async fn fetch_rates(now: i64) -> Result<RateTable, String> {
    // The TLS provider is process-wide; installing it twice is harmless.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;
    let response: RatesResponse = client
        .get(RATES_URL)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if response.result != "success" || response.rates.is_empty() {
        return Err("unexpected response".to_string());
    }
    Ok(RateTable { updated: response.time_last_update_unix, fetched_at: now, rates: response.rates })
}

// ────────────────────── Conversions ──────────────────────

/// A question of the form "<amount> <from> in <to>".
struct Conversion {
    amount: f64,
    from: String,
    to: String,
}

fn parse_conversion(query: &str) -> Option<Conversion> {
    let lower = query.trim().to_lowercase();
    let (left, to) = [" in ", " to ", " as ", " = "].iter().find_map(|sep| lower.split_once(sep))?;
    let (left, to) = (left.trim(), to.trim());

    // "$120" and "120 $" both mean 120 dollars
    let (amount, from) = match left.chars().next().filter(|c| currency_alias(&c.to_string()).is_some()) {
        Some(symbol) => {
            let (amount, rest) = leading_number(left[symbol.len_utf8()..].trim_start())?;
            if !rest.trim().is_empty() {
                return None;
            }
            (amount, symbol.to_string())
        }
        None => {
            let (amount, rest) = leading_number(left)?;
            (amount, rest.trim().to_string())
        }
    };
    if from.is_empty() || to.is_empty() {
        return None;
    }
    Some(Conversion { amount, from, to: to.to_string() })
}

/// Whether answering `query` needs exchange rates.
pub fn needs_rates(query: &str) -> bool {
    parse_conversion(query).is_some_and(|c| currency_code(&c.from).is_some() && currency_code(&c.to).is_some())
}

fn convert(conversion: &Conversion, rates: Option<&RateTable>) -> Option<Answer> {
    let Conversion { amount, from, to } = conversion;

    if let (Some(source), Some(target)) = (find_unit(from), find_unit(to)) {
        if source.0 != target.0 {
            return None;
        }
        return Some(Answer {
            value: format!("{} {}", format_number(amount * source.2 / target.2), target.1),
            kind: "unit".to_string(),
            detail: format!("{} {}", format_number(*amount), source.1),
        });
    }

    if let (Some(source), Some(target)) = (temperature(from), temperature(to)) {
        return Some(Answer {
            value: format!("{} {}", format_number((target.2)((source.1)(*amount))), target.0),
            kind: "unit".to_string(),
            detail: format!("{} {}", format_number(*amount), source.0),
        });
    }

    let (source, target) = (currency_code(from)?, currency_code(to)?);
    let table = rates?;
    let (source_rate, target_rate) = (table.rates.get(&source)?, table.rates.get(&target)?);
    let date = chrono::DateTime::from_timestamp(table.updated, 0)
        .map(|d| d.format("%d %b %Y").to_string())
        .unwrap_or_default();
    Some(Answer {
        value: format!("{} {}", format_money(amount / source_rate * target_rate), target),
        kind: "currency".to_string(),
        detail: format!("{} {} · {}", format_number(*amount), source, date),
    })
}

/// Answer `query` if it is a calculation or a conversion.
pub fn evaluate(query: &str, rates: Option<&RateTable>) -> Option<Answer> {
    if let Some(value) = arithmetic(query) {
        return Some(Answer { value: format_number(value), kind: "calculator".to_string(), detail: String::new() });
    }
    let lower = query.trim().to_lowercase();
    percent_of(&lower).or_else(|| convert(&parse_conversion(query)?, rates))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(query: &str) -> Option<String> {
        evaluate(query, None).map(|a| a.value)
    }

    fn rates() -> RateTable {
        RateTable {
            updated: 1_791_072_000,
            fetched_at: 0,
            rates: HashMap::from([("USD".to_string(), 1.0), ("INR".to_string(), 80.0), ("EUR".to_string(), 0.5)]),
        }
    }

    #[test]
    fn arithmetic_basics() {
        assert_eq!(value("2+2"), Some("4".to_string()));
        assert_eq!(value("10 * 5"), Some("50".to_string()));
        assert_eq!(value("100 / 4"), Some("25".to_string()));
        assert_eq!(value("3.14 * 2"), Some("6.28".to_string()));
        assert_eq!(value("(2 + 3) * 4"), Some("20".to_string()));
        assert_eq!(value("10 + 5 * 2"), Some("20".to_string()));
        assert_eq!(value("100 / (2 + 3)"), Some("20".to_string()));
    }

    #[test]
    fn powers_bind_tighter_and_associate_right() {
        assert_eq!(value("2^10"), Some("1024".to_string()));
        assert_eq!(value("2 * 3^2"), Some("18".to_string()));
        assert_eq!(value("2^3^2"), Some("512".to_string()));
    }

    #[test]
    fn rejects_non_math_and_division_by_zero() {
        assert_eq!(value("hello"), None);
        assert_eq!(value(""), None);
        assert_eq!(value("abc + 2"), None);
        assert_eq!(value("5 / 0"), None);
        assert_eq!(value("report-2024"), None);
    }

    #[test]
    fn percentages() {
        let answer = evaluate("15% of 240", None).unwrap();
        assert_eq!(answer.value, "36");
        assert_eq!(answer.detail, "15% of 240");
        assert_eq!(value("12.5% of 80"), Some("10".to_string()));
    }

    #[test]
    fn unit_conversions() {
        assert_eq!(value("5 km to miles"), Some("3.1069 mi".to_string()));
        assert_eq!(value("5km in mi"), Some("3.1069 mi".to_string()));
        assert_eq!(value("1 inch in cm"), Some("2.54 cm".to_string()));
        assert_eq!(value("2 hours in minutes"), Some("120 min".to_string()));
        assert_eq!(value("1 GB in MB"), Some("1000 MB".to_string()));
        assert_eq!(value("150 lbs in kg"), Some("68.0389 kg".to_string()));
        assert_eq!(evaluate("5 km to miles", None).unwrap().detail, "5 km");
    }

    #[test]
    fn temperature_conversions() {
        assert_eq!(value("100 c in f"), Some("212 °F".to_string()));
        assert_eq!(value("32 fahrenheit to celsius"), Some("0 °C".to_string()));
        assert_eq!(value("0 c to kelvin"), Some("273.15 K".to_string()));
    }

    #[test]
    fn mismatched_or_unknown_units_give_no_answer() {
        assert_eq!(value("5 km in kg"), None);
        assert_eq!(value("5 bananas in apples"), None);
        assert_eq!(value("notes in documents"), None);
    }

    #[test]
    fn currency_conversions_use_the_rate_table() {
        let table = rates();
        let answer = evaluate("120 usd in inr", Some(&table)).unwrap();
        assert_eq!(answer.value, "9,600.00 INR");
        assert_eq!(answer.kind, "currency");
        assert!(answer.detail.starts_with("120 USD · "));

        assert_eq!(evaluate("$10 to eur", Some(&table)).unwrap().value, "5.00 EUR");
        assert_eq!(evaluate("1,000 rupees in dollars", Some(&table)).unwrap().value, "12.50 USD");
        // No rates yet (first use, offline): no answer rather than a wrong one
        assert_eq!(evaluate("120 usd in inr", None), None);
        assert_eq!(evaluate("120 usd in xyz", Some(&table)), None);
    }

    #[test]
    fn only_currency_questions_ask_for_rates() {
        assert!(needs_rates("120 usd in inr"));
        assert!(needs_rates("$5 to eur"));
        assert!(!needs_rates("5 km to mi"));
        assert!(!needs_rates("2+2"));
        assert!(!needs_rates("photoshop"));
    }

    #[test]
    fn money_is_grouped() {
        assert_eq!(format_money(1234567.891), "1,234,567.89");
        assert_eq!(format_money(12.0), "12.00");
        assert_eq!(format_money(-1500.5), "-1,500.50");
    }
}
