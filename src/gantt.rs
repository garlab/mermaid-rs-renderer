//! Calendar and dependency calculations for the native, day-based Gantt subset.
//! Integer civil days keep the result independent of timezone, DST and the clock.

use std::collections::{BTreeSet, HashMap, VecDeque};

use anyhow::{Result, bail, ensure};

use crate::ir::Graph;

pub(crate) const MAX_TASKS: usize = 512;
pub(crate) const MAX_SPAN: i32 = 36_600;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Timing {
    pub start: i32,
    pub end: i32,
    // Mermaid trims trailing excluded days from the visible bar, while the
    // dependency still starts on the next included day.
    pub render_end: i32,
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

pub(crate) fn format_date(days: i32, format: &str) -> Result<String> {
    ensure!(format.len() <= 128, "Gantt axis format is too long");
    let (y, m, d) = civil_from_days(days);
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
            Some('Y') => output.push_str(&format!("{y:04}")),
            Some('y') => output.push_str(&format!("{:02}", y % 100)),
            Some('m') => output.push_str(&format!("{m:02}")),
            Some('d') => output.push_str(&format!("{d:02}")),
            Some('e') => output.push_str(&format!("{d:2}")),
            Some('b') => output.push_str(months[m as usize - 1]),
            Some('%') => output.push('%'),
            _ => bail!("unsupported Gantt axis format"),
        }
    }
    Ok(output)
}

// Only integral calendar units are accepted. In particular, `m` is minutes,
// never months; sub-day and fractional durations must not silently change dates.
fn duration_end(start: i32, duration: &str) -> Result<i32> {
    let split = duration
        .len()
        .checked_sub(1)
        .ok_or_else(|| anyhow::anyhow!("missing Gantt duration"))?;
    ensure!(duration.is_ascii(), "invalid Gantt duration");
    let (digits, unit) = duration.split_at(split);
    ensure!(
        !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        "unsupported Gantt duration"
    );
    let amount: i32 = digits.parse()?;
    ensure!(amount <= MAX_SPAN, "Gantt duration exceeds limit");
    let end = match unit {
        "d" => start + amount,
        "w" => start + amount * 7,
        "M" | "y" => {
            let (y, m, d) = civil_from_days(start);
            let months = y * 12 + m as i32 - 1 + amount * if unit == "y" { 12 } else { 1 };
            let year = months / 12;
            let month = (months % 12 + 1) as u32;
            ensure!(year <= 9999, "Gantt date exceeds limit");
            days_from_civil(year, month, d.min(month_days(year, month)))
        }
        _ => bail!("Gantt supports integral d, w, M and y durations"),
    };
    ensure!(end - start <= MAX_SPAN, "Gantt duration exceeds limit");
    Ok(end)
}

struct Calendar {
    weekends: bool,
    holidays: BTreeSet<i32>,
}

impl Calendar {
    fn weekend(day: i32) -> bool {
        (day + 3).rem_euclid(7) >= 5 // 1970-01-01 was Thursday.
    }

    fn excluded(&self, day: i32) -> bool {
        (self.weekends && Self::weekend(day)) || self.holidays.contains(&day)
    }

    fn included_days(&self, start: i32, end: i32) -> i32 {
        let span = end - start;
        let mut count = span;
        if self.weekends {
            count -= (span / 7) * 2;
            for offset in 1..=span % 7 {
                count -= i32::from(Self::weekend(start + offset));
            }
        }
        count
            - self
                .holidays
                .range((start + 1)..=end)
                .filter(|&&d| !self.weekends || !Self::weekend(d))
                .count() as i32
    }

    fn extend(&self, start: i32, nominal_end: i32) -> Result<(i32, i32)> {
        let duration = nominal_end - start;
        if duration == 0 || (!self.weekends && self.holidays.is_empty()) {
            return Ok((nominal_end, nominal_end));
        }
        // Count weekdays arithmetically, then binary-search the endpoint. Work
        // depends on the bounded holiday list, not the length of a date range.
        let (mut lo, mut hi) = (nominal_end, start + MAX_SPAN);
        ensure!(
            self.included_days(start, hi) >= duration,
            "Gantt calendar exceeds span limit"
        );
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.included_days(start, mid) < duration {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let mut render_end = lo;
        while render_end > start + 1 && self.excluded(render_end - 1) {
            render_end -= 1;
        }
        Ok((lo, render_end))
    }
}

pub(crate) fn schedule(graph: &Graph) -> Result<Vec<Timing>> {
    let tasks = &graph.gantt_tasks;
    ensure!(
        !tasks.is_empty() && tasks.len() <= MAX_TASKS,
        "Gantt needs 1..={MAX_TASKS} tasks"
    );
    ensure!(
        graph.gantt_excluded_dates.len() <= 128,
        "too many Gantt excluded dates"
    );
    let calendar = Calendar {
        weekends: graph.gantt_excludes_weekends,
        holidays: graph
            .gantt_excluded_dates
            .iter()
            .map(|d| parse_date(d))
            .collect::<Result<_>>()?,
    };
    let mut ids = HashMap::with_capacity(tasks.len());
    for (i, task) in tasks.iter().enumerate() {
        ensure!(
            !task.id.is_empty() && ids.insert(task.id.as_str(), i).is_none(),
            "duplicate or empty Gantt task ID"
        );
    }
    let mut dependencies = vec![Vec::new(); tasks.len()];
    let mut dependents = vec![Vec::new(); tasks.len()];
    let mut pending = vec![0usize; tasks.len()];
    let mut edges = 0;
    for (i, task) in tasks.iter().enumerate() {
        ensure!(
            task.start.is_none() || task.after.is_none(),
            "Gantt task has conflicting starts"
        );
        if let Some(after) = &task.after {
            for id in after.split_whitespace() {
                let &dep = ids
                    .get(id)
                    .ok_or_else(|| anyhow::anyhow!("unknown Gantt dependency: {id}"))?;
                dependencies[i].push(dep);
                edges += 1;
                ensure!(edges <= 4096, "too many Gantt dependencies");
            }
            ensure!(!dependencies[i].is_empty(), "empty Gantt dependency");
        } else if task.start.is_none() {
            ensure!(
                i > 0,
                "first Gantt task needs an explicit date or dependency"
            );
            dependencies[i].push(i - 1);
        }
        for &dep in &dependencies[i] {
            dependents[dep].push(i);
        }
        pending[i] = dependencies[i].len();
    }
    let mut ready: VecDeque<_> = pending
        .iter()
        .enumerate()
        .filter_map(|(i, &n)| (n == 0).then_some(i))
        .collect();
    let mut timings = vec![Timing::default(); tasks.len()];
    let mut visited = 0;
    while let Some(i) = ready.pop_front() {
        let task = &tasks[i];
        let start = match &task.start {
            Some(date) => parse_date(date)?,
            None => dependencies[i]
                .iter()
                .map(|&d| timings[d].end)
                .max()
                .unwrap(),
        };
        let (end, render_end) = match (&task.end, &task.duration) {
            (Some(date), None) => {
                let end = parse_date(date)?;
                (end, end)
            }
            (None, Some(duration)) => calendar.extend(start, duration_end(start, duration)?)?,
            _ => bail!("Gantt task needs exactly one end date or duration"),
        };
        ensure!(
            end >= start && end - start <= MAX_SPAN && end <= days_from_civil(9999, 12, 31),
            "invalid or excessive Gantt date range"
        );
        timings[i] = Timing {
            start,
            end,
            render_end,
        };
        visited += 1;
        for &next in &dependents[i] {
            pending[next] -= 1;
            if pending[next] == 0 {
                ready.push_back(next);
            }
        }
    }
    ensure!(visited == tasks.len(), "cyclic Gantt dependencies");
    let first = timings.iter().map(|t| t.start).min().unwrap();
    let last = timings.iter().map(|t| t.end).max().unwrap();
    ensure!(last - first <= MAX_SPAN, "Gantt chart exceeds span limit");
    Ok(timings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dates(source: &str) -> Vec<(String, String, String)> {
        let graph = crate::parse_mermaid_strict(source).unwrap().graph;
        schedule(&graph)
            .unwrap()
            .iter()
            .map(|t| {
                let date = |d| format_date(d, "%Y-%m-%d").unwrap();
                (date(t.start), date(t.end), date(t.render_end))
            })
            .collect()
    }

    #[test]
    fn implicit_tasks_follow_previous_end_even_across_sections() {
        let result =
            dates("gantt\nA: a, 2024-02-28, 1d\nB: 2d\nsection Next\nC: 2024-03-05\nD: 1w");
        assert_eq!(result[1].0, "2024-02-29");
        assert_eq!(result[1].1, "2024-03-02");
        assert_eq!(result[2].0, "2024-03-02");
        assert_eq!(result[2].1, "2024-03-05");
        assert_eq!(result[3].1, "2024-03-12");
    }

    #[test]
    fn dependencies_are_case_sensitive_support_forward_references_and_take_latest_end() {
        let result = dates(
            "gantt\nJoin: after Build Docs, 2d\nBuild: Build, 2026-01-01, 4d\nDocs: Docs, 2026-01-02, 1d",
        );
        assert_eq!(result[0].0, "2026-01-05");
        assert_eq!(result[0].1, "2026-01-07");
    }

    #[test]
    fn exclusions_match_mermaid_scheduling_and_visible_bar_endpoints() {
        let result = dates(
            "gantt\nexcludes weekends\nA: a, 2026-01-02, 1d\nB: after a, 5d\nC: milestone, after a, 0d\nD: 2026-01-02, 2026-01-04",
        );
        assert_eq!(
            result[0],
            (
                "2026-01-02".into(),
                "2026-01-05".into(),
                "2026-01-03".into()
            )
        );
        assert_eq!(
            result[1],
            (
                "2026-01-05".into(),
                "2026-01-12".into(),
                "2026-01-10".into()
            )
        );
        assert_eq!(result[2].0, result[2].1);
        assert_eq!(result[3].1, "2026-01-04", "explicit ends are not extended");
        let holidays =
            dates("gantt\nexcludes weekends\nexcludes 2026-01-05, 2026-01-06\nA: 2026-01-02, 1d");
        assert_eq!(holidays[0].1, "2026-01-07");
        assert_eq!(holidays[0].2, "2026-01-03");
    }

    #[test]
    fn calendar_months_years_and_leap_days_are_exact() {
        let result =
            dates("gantt\nMonth: 2024-01-31, 1M\nYear: 2024-02-29, 1y\nCentury: 2099-02-28, 1y");
        assert_eq!(result[0].1, "2024-02-29");
        assert_eq!(result[1].1, "2025-02-28");
        assert_eq!(result[2].1, "2100-02-28");
        for value in [
            "0001-01-01",
            "1900-03-01",
            "2000-02-29",
            "1970-01-01",
            "9999-12-31",
        ] {
            assert_eq!(
                format_date(parse_date(value).unwrap(), "%Y-%m-%d").unwrap(),
                value
            );
        }
    }

    #[test]
    fn unsupported_and_invalid_schedules_never_render_guessed_dates() {
        for body in [
            "A: 2d",
            "A: 2026-02-29, 1d",
            "A: 2026-04-31, 1d",
            "A: 2026-01-02, 2026-01-01",
            "A: 9999-12-31, 1d",
            "A: a, after b, 1d\nB: b, after a, 1d",
            "A: after missing, 1d",
            "A: a, 2026-01-01, 1d\nB: a, 2026-01-01, 1d",
            "A: Build, 2026-01-01, 1d\nB: after build, 1d",
            "A: 2026-01-01, 30m",
            "A: 2026-01-01, 24h",
            "A: 2026-01-01, 1.5d",
            "A: 2026-01-01, 1dX",
            "A: 2026-01-01, 999999999999999999d",
            "A: 2026-01-01, 36601d",
            "A: 2026-01-01, 99999999y",
            "dateFormat DD-MM-YYYY\nA: 2026-01-01, 1d",
            "axisFormat %H:%M\nA: 2026-01-01, 1d",
            "tickInterval 1day\nA: 2026-01-01, 1d",
            "includes 2026-01-01\nA: 2026-01-01, 1d",
            "todayMarker stroke:red\nA: 2026-01-01, 1d",
            "excludes weekdays\nA: 2026-01-01, 1d",
            "A: 2026-01-01, until other",
            "A: vert, v, 2026-01-01, 0d",
        ] {
            assert!(
                crate::parse_mermaid_strict(&format!("gantt\n{body}")).is_err(),
                "{body}"
            );
        }
    }

    #[test]
    fn iterative_resolution_bounds_deep_chains_and_calendar_work() {
        let mut source = "gantt\nexcludes weekends\nA: t0, 2026-01-01, 1d\n".to_string();
        for i in 1..MAX_TASKS {
            source.push_str(&format!("Task {i}: t{i}, after t{}, 1d\n", i - 1));
        }
        assert_eq!(dates(&source).len(), MAX_TASKS);
        source.push_str("Extra: 1d\n");
        assert!(crate::parse_mermaid_strict(&source).is_err());
        assert!(
            crate::parse_mermaid_strict("gantt\nexcludes weekends\nA: 2026-01-01, 36600d").is_err()
        );
    }

    #[test]
    fn arithmetic_calendar_matches_a_day_by_day_reference() {
        // Small exhaustive comparison with a deliberately simple oracle. This
        // catches weekend boundaries and trailing excluded bars independently
        // of the production binary search and whole-week arithmetic.
        for weekends in [false, true] {
            let calendar = Calendar {
                weekends,
                holidays: [10, 11, 18, 24].into(),
            };
            for start in 0..21 {
                for duration in 0..30 {
                    let (mut end, mut render, mut invalid) =
                        (start + duration, start + duration, false);
                    let mut cursor = start + 1;
                    while cursor <= end {
                        if !invalid {
                            render = end;
                        }
                        invalid = calendar.excluded(cursor);
                        if invalid {
                            end += 1;
                        }
                        cursor += 1;
                    }
                    assert_eq!(
                        calendar.extend(start, start + duration).unwrap(),
                        (end, render),
                        "{weekends}, {start}, {duration}"
                    );
                }
            }
        }
    }
}
