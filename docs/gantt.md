# Native day-based Gantt

The Gantt parser validates dates and resolves the complete schedule before
layout. Unsupported statements return an error so embedding applications can
retain source rather than display guessed dates. Rendering uses the existing
SVG backend and adds no dependency or runtime installation requirement.

## Supported syntax

- Titles, sections, task IDs and labels, including Unicode covered by the caller's fonts.
- `dateFormat YYYY-MM-DD` (also the default), with strict Gregorian dates in years 0001–9999.
- Explicit start/end dates; integral `d`, `w`, `M` and `y` durations. Months and
  years use calendar arithmetic and clamp to the last valid day of the month.
- Omitted starts continue from the preceding task's end, including across sections.
  The first task must have an explicit date or resolve through a dependency.
- `after ID` and `after ID1 ID2` use the latest dependency end. IDs are case
  sensitive; forward references work. Unknown IDs, duplicates and cycles fail.
- Independent `done`, `active`, `crit` and `milestone` tags, including combinations.
  Milestones appear at the interval midpoint; `0d` is an exact point.
- `excludes weekends` and explicit ISO holiday dates, separated by spaces or
  commas. Repeated `excludes` lines accumulate. Weekends are Saturday/Sunday.
- `axisFormat` with `%Y`, `%y`, `%m`, `%d`, `%e`, `%b`, `%%` and literal text.
  Month abbreviations are English. Tick spacing adapts to label width.
- `todayMarker off`. The native renderer does not draw a current-date marker.
- The existing `displayMode: compact` front matter remains available to direct
  backend callers; consumers may reject front matter independently.

Sub-day/fractional durations, other date formats, weekday exclusions, custom
weekends, `includes`, `until`, `tickInterval`, `weekday`, `inclusiveEndDates`,
`topAxis`, vertical markers, current-date styling and click handlers return an
error. In particular, `m` means minutes and is rejected; it is never treated as
months. This is a bounded native subset, not full Mermaid.js compatibility.

## Schedule and limits

`src/gantt.rs` owns integer civil dates, calendar calculations and iterative
dependency resolution. `src/parser/gantt.rs` owns syntax. Layout and SVG drawing
consume the validated dates; they do not substitute arbitrary durations or the
current date for invalid input. Programmatically constructed invalid graphs
produce an error layout.

Schedules allow at most 512 tasks, 4,096 explicit dependency references, 128
sections and 128 excluded-date entries. Each task and the complete chart span
at most 36,600 calendar days. Axis formats are limited to 128 bytes. Consumers
can impose smaller source, task, label and raster limits.

Dependency resolution uses a queue, not recursion. Weekend counting uses whole
weeks; a binary search finds excluded-day endpoints without walking every day
of a long schedule. Holiday storage and scan work are bounded by the date limit.
There is no timezone lookup or access to the clock.

Exclusions follow Mermaid's endpoint semantics: duration-based tasks extend
their dependency end to an included day, while the visible bar omits trailing
excluded days. Explicit end dates remain exact. Excluded dates are not shaded
as separate chart columns in this version.

Reference behavior is documented in the [official syntax guide](https://mermaid.js.org/syntax/gantt.html)
and the upstream [Gantt date calculations](https://github.com/mermaid-js/mermaid/blob/develop/packages/mermaid/src/diagrams/gantt/ganttDb.js).
Tests check explicit expected dates, leap years and month ends, combined tags,
milestone positions, invalid schedules and long chains. An independent small
day-by-day oracle checks the arithmetic calendar across weekend and holiday boundaries.

The Gantt IR now represents `GanttStatus` as independent boolean flags and adds
`GanttTask::end` plus graph calendar/axis fields. Callers constructing Gantt
structs directly must initialize these fields; the parse/render façade is unchanged.
