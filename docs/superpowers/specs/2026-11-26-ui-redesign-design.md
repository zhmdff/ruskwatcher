# Dashboard redesign: design

## Intent

The dashboard should look symmetrical and polished, with clear labels, icons, a clear layout and separate pages, including Settings (theme, language) and About.

Items marked *(proposed)* were filled in by the author, not requested.

## Goals

- Separate pages with sidebar navigation and icons.
- Balanced layout on every page, no orphaned cards.
- Full, plain labels.
- Settings page with theme and language.
- About page.
- All user-facing wording lives in the frontend. The database stores values only. English is the main language.

## Non-goals *(proposed)*

- A second language. The structure allows one; none is translated now.
- Right-to-left layout.
- New data collection, new detection rules, or new settings beyond theme and language.
- Update checks, external links.

## Anti-goals *(proposed)*

- Table search, sort, filters or column visibility stop working, or saved table preferences are lost.
- A frontend framework, build step or new dependency.
- Old findings become unreadable.

## Approaches

| | Approach | Trade-off |
|---|---|---|
| A | Restyle inside `index.html` | Smallest change; one very large file |
| **B (chosen)** | Plain files, no build step | Keeps current setup; components written by hand |
| C | Svelte + Vite | Better component and translation tooling; adds Node build and a rewrite of the tables |

B was picked in discussion.

## Files

```
crates/app/ui/index.html   shell: sidebar and page containers
crates/app/ui/style.css    theme colors and layout
crates/app/ui/i18n.js      strings per language, finding sentences
crates/app/ui/icons.js     inline SVG icons
crates/app/ui/app.js       navigation, rendering, charts, settings
crates/app/ui/table.js     existing; its own strings come from i18n
```

## Layout *(proposed)*

```
+----------------+------------------------------------------------+
| RuskWatcher    |  [icon] Overview                 [1 hour  v]   |
|                |  banner (only when something is wrong)         |
| [i] Overview   |  +----------------------+--------------------+ |
| [i] Apps       |  | CPU                  | Memory             | |
| [i] Findings   |  +--------------+-------+------+-------------+ |
| [i] Startup    |  | GPU          | Disk         | Network     | |
| [i] Drives     |  +--------------+--------------+-------------+ |
|                |                                                |
| [i] Settings   |                                                |
| [i] About      |                                                |
| updated 12:00  |                                                |
+----------------+------------------------------------------------+
```

- Narrow window: sidebar shrinks to icons only (each keeps a tooltip and accessible name); charts stack in one column.
- Every page has the same header (icon, title) and the same centered content width.
- Time range selector appears only on Overview, Apps and Findings, the pages it affects. One shared value.
- Selected page is marked for screen readers (`aria-current`).

## Pages *(proposed)*

| Page | Content |
|---|---|
| Overview | Five chart cards: current value, peak, value-axis maximum, start and end time |
| Apps | The apps table, moved off Overview, with the note that averages cover active samples only |
| Findings | Findings table |
| Startup | Startup entries, recent changes, boots |
| Drives | Drives table |
| Settings | Appearance (theme), Language, Background collector (existing controls) |
| About | Name, version, what the app does, collector and window roles, data file path, license (MIT), publisher |

Labels: "Sev" becomes "Severity", "Net" becomes "Network", "Disk" becomes "Disk traffic", and so on. Disk and Network cards state the admin/service requirement as a note, not in the title.

## Settings

- Theme: System (default), Light, Dark *(proposed)*. Applied immediately, remembered on this PC (`localStorage`). Status and chart colors must be readable in both themes.
- Language: selector listing English only for now *(proposed)*. Remembered the same way. An unknown saved value falls back to English; a missing string falls back to English.

## Values only from the backend

- `service`: stop writing the English `summary`; `detail_json` becomes `{}` (column is required). Service log lines stay English.
- `store`: the dashboard query drops `summary`. It already returns `kind`, `severity`, `app`, `value`, `threshold`.
- `i18n.js`: builds the sentence from `kind`, `value`, `app`. Window lengths (20 s for CPU rules, 10 s for memory and disk) are fixed per kind there.
- Old records use the same columns, so they render the same way.
- If `value` is missing (its snapshot was pruned), show the sentence without the number. If `app` is missing, say "unknown". An unknown `kind` shows as the raw kind.
- Finding kinds, startup change types, drive and boot values are translated in the frontend; unrecognised ones show raw.
- App commands return error codes (no data yet, service program not found, cancelled or failed) with any path as a value; the frontend words them.
- `service_status` also returns the app version, for About.

## Checks

- Existing Node and Rust tests pass.
- Node test: finding sentences, including missing value, missing app, unknown kind; theme and language fallback.
- Manual: each page in both themes, at default and minimum window size.

## Left to the builder

Colors, spacing, icon drawings, exact wording, how a language change redraws the page.
