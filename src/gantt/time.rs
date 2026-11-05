//! Strict, timezone-free timestamps and duration arithmetic for Gantt charts.

use anyhow::{Result, bail, ensure};
use std::fmt::Write;

pub(crate) const SECOND: i64 = 1_000;
pub(crate) const MINUTE: i64 = 60 * SECOND;
pub(crate) const HOUR: i64 = 60 * MINUTE;
pub(crate) const DAY: i64 = 24 * HOUR;
pub(crate) const MAX_SPAN_MS: i64 = super::MAX_SPAN as i64 * DAY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Clock {
    None,
    Minutes,
    Seconds,
    Milliseconds,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DateFormat {
    dated: bool,
    separator: u8,
    clock: Clock,
}

impl DateFormat {
    pub(crate) fn parse(value: &str) -> Result<Self> {
        let (dated, separator, clock) = if value == "YYYY-MM-DD" {
            return Ok(Self {
                dated: true,
                separator: b' ',
                clock: Clock::None,
            });
        } else if let Some(rest) = value.strip_prefix("YYYY-MM-DD") {
            if let Some(clock) = rest.strip_prefix(' ') {
                (true, b' ', clock)
            } else if let Some(clock) = rest.strip_prefix("[T]").or_else(|| rest.strip_prefix('T'))
            {
                (true, b'T', clock)
            } else {
                bail!("unsupported Gantt dateFormat");
            }
        } else {
            (false, b' ', value)
        };
        let clock = match clock {
            "HH:mm" => Clock::Minutes,
            "HH:mm:ss" => Clock::Seconds,
            "HH:mm:ss.SSS" => Clock::Milliseconds,
            _ => bail!("unsupported Gantt dateFormat"),
        };
        Ok(Self {
            dated,
            separator,
            clock,
        })
    }

    pub(crate) fn has_date(self) -> bool {
        self.dated
    }

    pub(crate) fn precision(self) -> i64 {
        match self.clock {
            Clock::None => DAY,
            Clock::Minutes => MINUTE,
            Clock::Seconds => SECOND,
            Clock::Milliseconds => 1,
        }
    }

    pub(crate) fn timestamp(self, value: &str) -> Result<i64> {
        ensure!(value.is_ascii(), "invalid Gantt timestamp");
        let clock_len = match self.clock {
            Clock::None => 0,
            Clock::Minutes => 5,
            Clock::Seconds => 8,
            Clock::Milliseconds => 12,
        };
        let date_len = if self.dated {
            10 + usize::from(clock_len > 0)
        } else {
            0
        };
        ensure!(
            value.len() == date_len + clock_len,
            "Gantt timestamp does not match dateFormat"
        );
        let date = if self.dated {
            i64::from(parse_date(&value[..10])?) * DAY
        } else {
            0
        };
        if clock_len == 0 {
            return Ok(date);
        }
        if self.dated {
            ensure!(
                value.as_bytes()[10] == self.separator,
                "Gantt timestamp does not match dateFormat"
            );
        }
        let clock = &value[date_len..];
        ensure!(
            clock.bytes().enumerate().all(|(i, c)| match i {
                2 | 5 => c == b':',
                8 => c == b'.',
                _ => c.is_ascii_digit(),
            }),
            "invalid Gantt time"
        );
        let hour: i64 = clock[..2].parse()?;
        let minute: i64 = clock[3..5].parse()?;
        let second: i64 = if clock_len >= 8 {
            clock[6..8].parse()?
        } else {
            0
        };
        let millis: i64 = if clock_len == 12 {
            clock[9..].parse()?
        } else {
            0
        };
        ensure!(
            hour < 24 && minute < 60 && second < 60,
            "invalid Gantt time"
        );
        Ok(date + hour * HOUR + minute * MINUTE + second * SECOND + millis)
    }
}

pub(crate) fn last_timestamp() -> i64 {
    (i64::from(days_from_civil(9999, 12, 31)) + 1) * DAY - 1
}

pub(crate) fn looks_like_duration(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && value.ends_with(['d', 'w', 'M', 'y', 'h', 'm', 's'])
}

/// Parse a nonnegative decimal exactly, rejecting fractional milliseconds and
/// overflow. Limiting decimal places also bounds hostile numeric input work.
fn scaled_decimal(value: &str, scale: i64, fractional: bool) -> Result<i64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    ensure!(
        !whole.is_empty() && whole.bytes().all(|c| c.is_ascii_digit()),
        "invalid Gantt duration"
    );
    ensure!(
        !value.contains('.') || (fractional && !fraction.is_empty()),
        "unsupported fractional Gantt duration"
    );
    ensure!(
        fraction.len() <= 9 && fraction.bytes().all(|c| c.is_ascii_digit()),
        "invalid Gantt duration precision"
    );
    let whole: u64 = whole.parse()?;
    let mut amount = u128::from(whole) * scale as u128;
    if !fraction.is_empty() {
        let numerator = u128::from(fraction.parse::<u64>()?) * scale as u128;
        let denominator = 10u128.pow(fraction.len() as u32);
        ensure!(
            numerator.is_multiple_of(denominator),
            "Gantt duration needs whole milliseconds"
        );
        amount += numerator / denominator;
    }
    ensure!(
        amount <= MAX_SPAN_MS as u128,
        "Gantt duration exceeds span limit"
    );
    Ok(amount as i64)
}

pub(crate) fn duration_end(start: i64, value: &str, dated: bool) -> Result<i64> {
    ensure!(value.is_ascii(), "invalid Gantt duration");
    let (number, unit) = if let Some(number) = value.strip_suffix("ms") {
        (number, "ms")
    } else if !value.is_empty() {
        value.split_at(value.len() - 1)
    } else {
        bail!("missing Gantt duration");
    };
    let fixed = match unit {
        "ms" => Some(1),
        "s" => Some(SECOND),
        "m" => Some(MINUTE),
        "h" => Some(HOUR),
        "d" => Some(DAY),
        "w" => Some(7 * DAY),
        _ => None,
    };
    if let Some(scale) = fixed {
        let delta = scaled_decimal(number, scale, matches!(unit, "ms" | "s" | "m" | "h"))?;
        return start
            .checked_add(delta)
            .ok_or_else(|| anyhow::anyhow!("Gantt timestamp overflow"));
    }
    ensure!(
        dated && matches!(unit, "M" | "y"),
        "unsupported Gantt duration or undated calendar unit"
    );
    let amount = scaled_decimal(number, 1, false)?;
    ensure!(
        amount <= i64::from(super::MAX_SPAN),
        "Gantt duration exceeds limit"
    );
    let (year, month, day) = civil_from_days(start.div_euclid(DAY) as i32);
    let months =
        i64::from(year) * 12 + i64::from(month) - 1 + amount * if unit == "y" { 12 } else { 1 };
    let year = months / 12;
    let month = (months % 12 + 1) as u32;
    ensure!(year <= 9999, "Gantt date exceeds limit");
    let date = days_from_civil(year as i32, month, day.min(month_days(year as i32, month)));
    let end = i64::from(date) * DAY + start.rem_euclid(DAY);
    ensure!(
        end - start <= MAX_SPAN_MS,
        "Gantt duration exceeds span limit"
    );
    Ok(end)
}

pub(crate) fn format_timestamp(timestamp: i64, format: &str, dated: bool) -> Result<String> {
    ensure!(format.len() <= 128, "Gantt axis format is too long");
    let time = timestamp.rem_euclid(DAY);
    let (year, month, day) = civil_from_days(timestamp.div_euclid(DAY) as i32);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut output = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            output.push(c);
            continue;
        }
        match chars.next() {
            Some('H') => write!(output, "{:02}", time / HOUR)?,
            Some('M') => write!(output, "{:02}", time / MINUTE % 60)?,
            Some('S') => write!(output, "{:02}", time / SECOND % 60)?,
            Some('L') => write!(output, "{:03}", time % SECOND)?,
            Some('%') => output.push('%'),
            Some('Y') if dated => write!(output, "{year:04}")?,
            Some('y') if dated => write!(output, "{:02}", year % 100)?,
            Some('m') if dated => write!(output, "{month:02}")?,
            Some('d') if dated => write!(output, "{day:02}")?,
            Some('e') if dated => write!(output, "{day:2}")?,
            Some('b') if dated => output.push_str(months[month as usize - 1]),
            _ => bail!("unsupported Gantt axis format for these dates"),
        }
    }
    Ok(output)
}

pub(crate) fn tick_times(start: i64, end: i64, intervals: i64, whole_days: bool) -> Vec<i64> {
    let span = end - start;
    let intervals = intervals
        .clamp(1, 4)
        .min(if whole_days { span / DAY } else { span })
        .max(1);
    if whole_days {
        // Retain existing day-chart ticks and geometry. Round relative offsets,
        // never large epoch timestamps converted to a low-precision float.
        let days = span / DAY;
        return (0..=intervals)
            .map(|i| start + ((days * i * 2 + intervals) / (2 * intervals)) * DAY)
            .collect();
    }
    let target = (span + intervals - 1) / intervals;
    let step = [
        1,
        2,
        5,
        10,
        20,
        50,
        100,
        200,
        500,
        SECOND,
        2 * SECOND,
        5 * SECOND,
        10 * SECOND,
        15 * SECOND,
        30 * SECOND,
        MINUTE,
        2 * MINUTE,
        5 * MINUTE,
        10 * MINUTE,
        15 * MINUTE,
        30 * MINUTE,
        HOUR,
        2 * HOUR,
        3 * HOUR,
        6 * HOUR,
        12 * HOUR,
        DAY,
    ]
    .into_iter()
    .find(|&n| n >= target)
    .unwrap_or(((target + DAY - 1) / DAY) * DAY);
    let first = start.div_euclid(step) * step;
    let first = if first < start { first + step } else { first };
    let mut ticks = Vec::with_capacity(5);
    let mut time = first;
    while time <= end && ticks.len() < 5 {
        ticks.push(time);
        time += step;
    }
    if ticks.is_empty() {
        // A wide label can require a step longer than the chart's range. Keep
        // an axis even when no aligned clock tick falls inside that range.
        ticks.extend([start, end]);
    }
    ticks
}

pub(crate) fn parse_date(value: &str) -> Result<i32> {
    let bytes = value.as_bytes();
    ensure!(
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()),
        "Gantt dates must use YYYY-MM-DD"
    );
    let year: i32 = value[..4].parse()?;
    let month: u32 = value[5..7].parse()?;
    let day: u32 = value[8..].parse()?;
    ensure!(
        (1..=9999).contains(&year)
            && (1..=12).contains(&month)
            && (1..=month_days(year, month)).contains(&day),
        "invalid Gantt date: {value}"
    );
    Ok(days_from_civil(year, month, day))
}

fn month_days(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i32 {
    let y = year - i32::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month as i32;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

fn civil_from_days(days: i32) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i32::from(m <= 2), m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_duration_units_and_fractional_hours_minutes_seconds() {
        for (value, expected) in [
            ("2h", 2 * HOUR),
            ("1.5h", 90 * MINUTE),
            ("0.125m", 7500),
            ("1.001s", 1001),
            ("0.000005h", 18),
            ("500ms", 500),
            ("1.0ms", 1),
            ("0ms", 0),
        ] {
            assert_eq!(duration_end(0, value, false).unwrap(), expected, "{value}");
        }
        for value in [
            "0.1ms",
            "0.0001s",
            "0.5d",
            "1.5w",
            "1.0M",
            "1M",
            "1y",
            "-1h",
            "+1h",
            "NaNh",
            "1e3s",
            "99999999999999999999999999999999999h",
            "24H",
            "1.s",
            "1.0000000000s",
            "0.000000001h",
            "é",
            "",
            "1.2.3m",
        ] {
            assert!(duration_end(0, value, false).is_err(), "accepted {value}");
        }
    }

    #[test]
    fn formats_are_strict_and_do_not_depend_on_the_clock() {
        let values = [
            ("HH:mm", "09:30", 9 * HOUR + 30 * MINUTE),
            ("HH:mm:ss", "09:30:45", 9 * HOUR + 30 * MINUTE + 45 * SECOND),
            (
                "HH:mm:ss.SSS",
                "09:30:45.123",
                9 * HOUR + 30 * MINUTE + 45123,
            ),
        ];
        for (format, value, expected) in values {
            assert_eq!(
                DateFormat::parse(format).unwrap().timestamp(value).unwrap(),
                expected
            );
            for separator in [" ", "T", "[T]"] {
                let date_format =
                    DateFormat::parse(&format!("YYYY-MM-DD{separator}{format}")).unwrap();
                let value_separator = if separator == "[T]" { "T" } else { separator };
                assert_eq!(
                    date_format
                        .timestamp(&format!("1970-01-01{value_separator}{value}"))
                        .unwrap(),
                    expected
                );
            }
        }
        let format = DateFormat::parse("HH:mm:ss.SSS").unwrap();
        for invalid in [
            "24:00:00.000",
            "12:60:00.000",
            "12:00:60.000",
            "1:00:00.000",
            "12:00:00.0",
            "12:00:00.0000",
            "12:00:00.000Z",
            "12:00:00.000+01:00",
            "中文",
        ] {
            assert!(format.timestamp(invalid).is_err(), "accepted {invalid}");
        }
        for format in [
            "YYYY-MM-DD HH:mm Z",
            "hh:mm A",
            "HH:mm:ss.SSSS",
            "DD-MM-YYYY",
            "X",
        ] {
            assert!(DateFormat::parse(format).is_err(), "accepted {format}");
        }
    }

    #[test]
    fn negative_epoch_and_end_of_calendar_preserve_milliseconds() {
        let format = DateFormat::parse("YYYY-MM-DD HH:mm:ss.SSS").unwrap();
        for value in [
            "0001-01-01 00:00:00.001",
            "1969-12-31 23:59:59.999",
            "1970-01-01 00:00:00.000",
            "9999-12-31 23:59:59.999",
        ] {
            let time = format.timestamp(value).unwrap();
            assert_eq!(
                format_timestamp(time, "%Y-%m-%d %H:%M:%S.%L", true).unwrap(),
                value
            );
        }
        assert_eq!(format.timestamp("1969-12-31 23:59:59.999").unwrap(), -1);
        assert_eq!(
            format.timestamp("9999-12-31 23:59:59.999").unwrap(),
            last_timestamp()
        );
        assert!(format_timestamp(0, "%Y-%m-%d", false).is_err());
    }

    #[test]
    fn ticks_are_bounded_monotone_and_keep_short_intervals_at_late_dates() {
        assert_eq!(
            tick_times(9 * HOUR + MINUTE, 10 * HOUR - MINUTE, 1, false),
            [9 * HOUR + MINUTE, 10 * HOUR - MINUTE]
        );
        let anchor = DateFormat::parse("YYYY-MM-DD HH:mm:ss.SSS")
            .unwrap()
            .timestamp("9999-01-01 09:30:00.000")
            .unwrap();
        for span in [1, 2, 3, 30, 1500, MINUTE, 8 * HOUR, 3 * DAY, MAX_SPAN_MS] {
            for count in 1..=4 {
                let ticks = tick_times(anchor, anchor + span, count, false);
                assert!(!ticks.is_empty() && ticks.len() <= 5);
                assert!(ticks.iter().all(|&t| (anchor..=anchor + span).contains(&t)));
                assert!(ticks.windows(2).all(|t| t[0] < t[1]));
            }
        }
        assert_eq!(
            tick_times(anchor, anchor + 3, 4, false),
            (anchor..=anchor + 3).collect::<Vec<_>>()
        );
    }
}
