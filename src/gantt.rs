//! Calendar and dependency calculations for native Gantt charts.
//! Integer milliseconds preserve short intervals independently of timezone and clock.

use std::collections::{BTreeSet, HashMap, VecDeque};

use anyhow::{Result, bail, ensure};

use crate::ir::Graph;

pub(crate) mod time;
use time::{DAY, DateFormat, MAX_SPAN_MS, duration_end, parse_date};

pub(crate) const MAX_TASKS: usize = 512;
pub(crate) const MAX_SPAN: i32 = 36_600;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Timing {
    pub start: i64,
    pub end: i64,
    // Mermaid trims trailing excluded days from the visible bar, while the
    // dependency still starts on the next included day.
    pub render_end: i64,
}

struct Calendar {
    weekends: bool,
    holidays: BTreeSet<i32>,
}

impl Calendar {
    fn extend_timestamp(&self, start: i64, nominal_end: i64) -> Result<(i64, i64)> {
        let duration = nominal_end - start;
        ensure!(
            (0..=MAX_SPAN_MS).contains(&duration),
            "Gantt duration exceeds span limit"
        );
        let day = start.div_euclid(DAY) as i32;
        // The reference renderer checks excluded dates at whole-day offsets
        // from the start's clock time. Preserve its fractional-day remainder.
        let (end, render_end) = self.extend(day, day + (duration / DAY) as i32)?;
        let remainder = duration % DAY;
        Ok((
            start + i64::from(end - day) * DAY + remainder,
            start + i64::from(render_end - day) * DAY + remainder,
        ))
    }

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
    let format = DateFormat::parse(graph.gantt_date_format.as_deref().unwrap_or("YYYY-MM-DD"))?;
    if let Some(axis) = &graph.gantt_axis_format {
        time::format_timestamp(0, axis, format.has_date())?;
    }
    ensure!(
        format.has_date()
            || (!graph.gantt_excludes_weekends && graph.gantt_excluded_dates.is_empty()),
        "calendar exclusions need dated Gantt tasks"
    );
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
            Some(date) => format.timestamp(date)?,
            None => dependencies[i]
                .iter()
                .map(|&d| timings[d].end)
                .max()
                .unwrap(),
        };
        let (end, render_end) = match (&task.end, &task.duration) {
            (Some(date), None) => {
                let end = format.timestamp(date)?;
                (end, end)
            }
            (None, Some(duration)) => calendar
                .extend_timestamp(start, duration_end(start, duration, format.has_date())?)?,
            _ => bail!("Gantt task needs exactly one end date or duration"),
        };
        ensure!(
            end >= start && end - start <= MAX_SPAN_MS && end <= time::last_timestamp(),
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
    ensure!(
        last - first <= MAX_SPAN_MS,
        "Gantt chart exceeds span limit"
    );
    Ok(timings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::format_timestamp;

    fn moments(source: &str) -> Vec<Timing> {
        let graph = crate::parse_mermaid_strict(source).unwrap().graph;
        schedule(&graph).unwrap()
    }

    #[test]
    fn sub_day_dependencies_explicit_ends_and_overnight_durations() {
        use time::{HOUR, MINUTE};
        let result = moments(
            "gantt\ndateFormat HH:mm\nBuild: a, 09:00, 1.5h\nCheck: after a, 15m\nEnd: milestone, 0ms\nParallel: 09:15, 10:00\nOvernight: 23:30, 90m",
        );
        assert_eq!(
            (result[0].start, result[0].end),
            (9 * HOUR, 10 * HOUR + 30 * MINUTE)
        );
        assert_eq!(result[1].end, 10 * HOUR + 45 * MINUTE);
        assert_eq!(result[2].end, result[1].end);
        assert_eq!(result[3].end, 10 * HOUR);
        assert_eq!(result[4].end, DAY + HOUR);
        let deferred = moments("gantt\nA: 09:00, 30m\ndateFormat HH:mm");
        assert_eq!(deferred[0].end, 9 * HOUR + 30 * MINUTE);
    }

    #[test]
    fn dated_times_cross_midnight_leap_days_and_exclusions_without_losing_clock_time() {
        let source = "gantt\ndateFormat YYYY-MM-DD HH:mm:ss.SSS\nA: 2024-02-28 23:59:59.999, 1ms\nB: 2024-02-29 12:30:00.125, 1y\nC: 2024-01-31 23:45:12.250, 1M";
        let result = moments(source);
        let format = |t| format_timestamp(t, "%Y-%m-%d %H:%M:%S.%L", true).unwrap();
        assert_eq!(format(result[0].end), "2024-02-29 00:00:00.000");
        assert_eq!(format(result[1].end), "2025-02-28 12:30:00.125");
        assert_eq!(format(result[2].end), "2024-02-29 23:45:12.250");
        let result = moments(
            "gantt\ndateFormat YYYY-MM-DD HH:mm\nexcludes weekends\nA: a, 2026-01-02 16:30, 25h\nB: after a, 30m",
        );
        assert_eq!(format(result[0].end), "2026-01-05 17:30:00.000");
        assert_eq!(format(result[0].render_end), "2026-01-03 17:30:00.000");
        assert_eq!(format(result[1].end), "2026-01-05 18:00:00.000");
    }

    #[test]
    fn millisecond_chains_are_exact_and_invalid_sub_day_schedules_keep_source() {
        let mut source =
            "gantt\ndateFormat YYYY-MM-DD HH:mm:ss.SSS\nA: a0, 9999-01-01 12:00:00.001, 1ms\n"
                .to_string();
        for i in 1..512 {
            source.push_str(&format!("Task {i}: a{i}, after a{}, 1ms\n", i - 1));
        }
        let result = moments(&source);
        assert_eq!(result[511].end - result[0].start, 512);
        for source in [
            "gantt\ndateFormat HH:mm\nA: 23:00, 00:30",
            "gantt\ndateFormat HH:mm\nexcludes weekends\nA: 09:00, 1h",
            "gantt\ndateFormat HH:mm\nA: 09:00, 1M",
            "gantt\ndateFormat HH:mm\naxisFormat %Y\nA: 09:00, 1h",
            "gantt\ndateFormat YYYY-MM-DD HH:mm:ss.SSS\nA: 9999-12-31 23:59:59.999, 1ms",
            "gantt\ndateFormat HH:mm\nA: 09:00, 3162240000001ms",
        ] {
            assert!(
                crate::parse_mermaid_strict(source).is_err(),
                "accepted {source}"
            );
        }
    }

    fn dates(source: &str) -> Vec<(String, String, String)> {
        let graph = crate::parse_mermaid_strict(source).unwrap().graph;
        schedule(&graph)
            .unwrap()
            .iter()
            .map(|t| {
                let date = |d| format_timestamp(d, "%Y-%m-%d", true).unwrap();
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
                format_timestamp(
                    i64::from(parse_date(value).unwrap()) * DAY,
                    "%Y-%m-%d",
                    true
                )
                .unwrap(),
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
            "A: 2026-01-01, 30q",
            "A: 2026-01-01, NaNh",
            "A: 2026-01-01, 1.5d",
            "A: 2026-01-01, 1dX",
            "A: 2026-01-01, 999999999999999999d",
            "A: 2026-01-01, 36601d",
            "A: 2026-01-01, 99999999y",
            "dateFormat DD-MM-YYYY\nA: 2026-01-01, 1d",
            "axisFormat %Z\nA: 2026-01-01, 1d",
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
