# Dashboard table controls: design

## Intent

Make the dashboard tables easy to use: users find rows by searching, filtering and sorting, and choose which columns they see.

## Goals

All six tables (apps, findings, startup entries, startup changes, boots, drives) get:

- **Search:** one box per table, case-insensitive, matches the displayed text of every column, hidden columns included.
- **Sort:** click a header to cycle ascending, descending, original order. Numbers, sizes, durations and dates sort by raw value, text by locale compare. Missing values go last in both directions.
- **Filters:** dropdowns on apps (Signed), findings (Kind, Sev), startup entries (Source, State), startup changes (Change), drives (Type). Options come from the loaded rows; a selected option stays listed even if no row currently has it. Search and filters combine (all must match).
- **Column visibility:** a "Columns" menu per table. The first column cannot be hidden.

## Non-goals

Column reorder or resize, user-defined tables, pagination, export, multi-column sort, sticky headers.

## Anti-goals

- The 5 s refresh must not clear search text, steal focus, close an open menu, or reset sort and filters.
- Sorting must never rank only a truncated subset by some other measure (see row limits).

## Constraints

- Plain HTML, CSS and JS in `crates/app/ui`; no build step, no dependency, no CDN.
- The app stays read-only toward the database.
- Existing escaping of data values stays; saved settings and search text are never inserted as HTML.
- Light and dark themes both work; controls are keyboard usable and sorted headers set `aria-sort`.

## Approaches

1. **Extend the existing `table()` function (chosen).** Small, no dependency, matches the current look. No reorder or resize.
2. **Vendor Tabulator.** Brings reorder and resize, but adds a large file, theming work and refresh handling. Not picked.

## Design

- Each column has a stable id, label and alignment; each cell provides a raw sort value and display HTML.
- Controls are created once per table; refresh redraws only the table rows.
- `localStorage` saves hidden columns and sort per table. Unknown column ids and unreadable saved data fall back to defaults. Search and filters reset on launch.
- Empty states: "nothing yet" when there is no data, "no matches" when search or filters exclude every row.
- The apps card is titled "Apps in range" (it is no longer a top list).
- Row limits in `crates/store/src/query.rs`: apps unlimited (was 30), findings 500 (was 50), startup changes 200 (was 20), boots 50 (was 5). Default order stays as the query returns it. Search covers only these loaded rows.

## Checks

- Rust test: more than 30 active apps are all returned.
- A small self-check for the sort and filter logic, kept outside `crates/app/ui` so it is not bundled.
- Launch the app and try each control across a refresh.

## Left to the builder

File split, function names, control styling.
