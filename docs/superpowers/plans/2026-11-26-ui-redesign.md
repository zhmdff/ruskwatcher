# Dashboard Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sidebar dashboard with icons, balanced pages, Settings (theme, language) and About, with all wording in the frontend.

**Architecture:** Plain classic scripts in `crates/app/ui`, no build step; they share the global scope and export for Node tests through `if (typeof module !== "undefined")`. The backend returns values and error codes; `i18n.js` words them.

**Tech Stack:** HTML/CSS/vanilla JS, Tauri 2, Rust, rusqlite, `node --test`.

**Spec:** `docs/superpowers/specs/2026-11-26-ui-redesign-design.md`

## Global Constraints

- No frontend framework, build step or new dependency.
- English only ships; unknown saved language and missing strings fall back to English.
- Theme: `system` (default), `light`, `dark`; stored in `localStorage`; readable in both themes.
- Table search, sort, filters, column visibility and saved prefs keep working: table keys `apps findings startup changes boots drives` and column ids stay unchanged.
- `detail_json` is written as `{}`; service log lines stay English.
- Text from the database is never inserted as HTML unescaped.
- JS tests: `node --test "crates/app/ui-test/*.test.js"`.
- Commit steps run only if the user has authorized commits.

## Review Focus

- App name containing `{v}`, `$&` or `<b>`: shown literally (Task 1; table escaping already covers HTML).
- Corrupt or unknown saved theme/language, or `localStorage` throwing: defaults, app still starts (Task 1, Task 5).
- Backend value matching an object property (`toString`, `__proto__`): shown raw (Task 1).
- Error that is not a known code (SQLite message): shown as is (Task 1).
- Chart with no samples, all-null or one sample: shows `–` or the value, never `NaN`/`Infinity` (Task 4).

---

### Task 1: Strings and wording (`i18n.js`)

**Files:**
- Create: `crates/app/ui/i18n.js`
- Test: `crates/app/ui-test/i18n.test.js`

**Interfaces (Produces, all global):**
- `STRINGS = { en: {...} }`, flat dotted keys
- `pickLang(raw): string`, `pickTheme(raw): "system"|"light"|"dark"`
- `setLang(raw): void`
- `t(key, vars = {}): string` (current language, then `en`, then the key; `{name}` replaced in one pass)
- `tv(group, value): string` (`t(group + "." + value)` if that key exists, else `String(value)`)
- `findingText({kind, value, app}): string`
- `errorText(error): string` (`"code"` or `"code:path"`, split at the first colon)

- [ ] **Step 1: Write the failing test**

```js
const test = require("node:test");
const assert = require("node:assert/strict");
const { t, tv, pickLang, pickTheme, setLang, findingText, errorText } = require("../ui/i18n.js");
const f = (kind, value, app) => findingText({ kind, value, app });

test("finding sentences", () => {
  assert.equal(f("system_cpu_high", 93, null), "CPU averaged 93% over 20 s; busiest app unknown");
  assert.equal(f("app_cpu_high", 62.4, "chrome"), "chrome averaged 62% CPU over 20 s");
  assert.equal(f("memory_pressure", 91.6, "code"), "RAM 92% used; largest app code");
  assert.equal(f("disk_heavy", 157286400, "x"), "x moved 150 MB/s over 10 s");
});
test("missing value, unknown kind", () => {
  assert.equal(f("system_cpu_high", null, "a"), "CPU stayed high over 20 s; busiest app a");
  assert.equal(f("app_cpu_high", null, "a"), "a used a lot of CPU over 20 s");
  assert.equal(f("memory_pressure", null, "a"), "RAM was nearly full over 10 s; largest app a");
  assert.equal(f("disk_heavy", null, "a"), "a moved a lot of disk data over 10 s");
  assert.equal(f("new_rule", 1, "a"), "new_rule");
  assert.equal(f("toString", 1, "a"), "toString");
});
test("values are literal", () => {
  assert.equal(f("app_cpu_high", 60, "{v}$&<b>"), "{v}$&<b> averaged 60% CPU over 20 s");
  assert.equal(tv("change", "__proto__"), "__proto__");
  assert.equal(t("no.such.key"), "no.such.key");
});
test("preferences fall back", () => {
  for (const raw of [null, "", "xx", "toString"]) {
    assert.equal(pickLang(raw), "en");
    assert.equal(pickTheme(raw), "system");
  }
  assert.equal(pickTheme("light"), "light");
  assert.equal(pickTheme("dark"), "dark");
  setLang("xx");
  assert.equal(t("nav.about"), "About");
});
test("errors", () => {
  assert.ok(errorText("no_data:C:\\d\\r.db").includes("C:\\d\\r.db"));
  assert.ok(errorText("service_exe_missing:C:\\a\\service.exe").includes("C:\\a\\service.exe"));
  assert.notEqual(errorText("elevation_failed"), "elevation_failed");
  assert.equal(errorText("database is locked"), "database is locked");
});
```

- [ ] **Step 2:** Run the JS tests. Expected: FAIL, cannot find `../ui/i18n.js`.
- [ ] **Step 3: Implement.** Use `Object.hasOwn` for every lookup. Keys: `nav.<page>` (Overview, Apps, Findings, Startup, Drives, Settings, About), `kind.<kind>` for the four kinds, `change.added|changed|removed`, severity `sev.1` "Warning" and `sev.2` "High", `finding.<kind>` and `finding.<kind>.noValue` with the sentences above, `error.<code>`. `no_data` keeps the meaning "No data at {path} yet. Is the RuskWatcher service running?". Later tasks add their own keys here.
- [ ] **Step 4:** Run the JS tests. Expected: PASS.
- [ ] **Step 5:** Commit `feat: add frontend strings and finding wording`.

### Task 2: Backend returns values and codes

**Files:**
- Modify: `crates/store/src/query.rs` (findings query), `crates/service/src/run.rs:245`, `crates/app/src/main.rs`
- Test: test modules in `crates/store/src/lib.rs` and `crates/app/src/main.rs`

**Interfaces (Produces):**
- `findings[]`: `{id, ts, kind, severity, app, value, threshold}`, no `summary`
- Command errors: `no_data:<db path>`, `service_exe_missing:<exe path>`, `elevation_failed`; other errors unchanged
- `service_status` adds `version: env!("CARGO_PKG_VERSION")`

- [ ] **Step 1: Write the failing tests.** In `store/src/lib.rs`:

```rust
#[test]
fn dashboard_findings_carry_values_not_text() {
    let mut s = mem();
    let procs = vec![app("hog.exe", 90.0)];
    s.insert_finding(&FindingInput {
        ts: 9000,
        kind: "app_cpu_high",
        severity: 1,
        metric: "app_cpu_percent",
        value: 95.0,
        threshold: 50.0,
        pre_window_sec: 30,
        sys_json: "{}",
        detail_json: r#"{"summary":"old text"}"#,
        procs: &procs,
        culprit: Some("hog.exe"),
    })
    .unwrap();
    let v = s.dashboard(3600, 10_000).unwrap();
    let f = &v["findings"][0];
    assert!(f.get("summary").is_none());
    assert_eq!(f["value"], 95.0);
    assert_eq!(f["threshold"], 50.0);
    assert!(f["app"].is_string());
}
```

In `app/src/main.rs` tests:

```rust
#[test]
fn missing_db_is_a_code() {
    std::env::set_var("RUSKWATCHER_DB", r"Z:\nope\x.db");
    assert_eq!(super::dashboard(60).unwrap_err(), r"no_data:Z:\nope\x.db");
}

#[test]
fn status_has_version() {
    assert_eq!(super::service_status()["version"], env!("CARGO_PKG_VERSION"));
}
```

- [ ] **Step 2:** Run `cargo test -p store -p ruskwatcher-app`. Expected: the three new tests FAIL.
- [ ] **Step 3: Implement.** Drop `f.detail_json` from the select (value and threshold column indices move to 5 and 6). In `run.rs` pass `detail_json: "{}"`; keep `f.detail` for the log line.
- [ ] **Step 4:** Run `cargo test`. Expected: PASS, no new warnings.
- [ ] **Step 5:** Commit `refactor: backend returns values and error codes`.

### Task 3: Icons

**Files:**
- Create: `crates/app/ui/icons.js`

**Interfaces (Produces):** global `icon(name): string`, an inline `<svg class="icon" aria-hidden="true">` using `currentColor`; unknown name returns `""`. Names: `overview apps findings startup drives settings about cpu memory gpu disk network`.

- [ ] **Step 1:** Implement with a name-to-path map and `Object.hasOwn`; export via `module.exports`.
- [ ] **Step 2:** Run `node -e "const {icon}=require('./crates/app/ui/icons.js'); for (const n of 'overview apps findings startup drives settings about cpu memory gpu disk network'.split(' ')) if(!icon(n).startsWith('<svg')) throw n; if(icon('toString')!=='') throw 'unknown'"`. Expected: exit 0, no output.
- [ ] **Step 3:** Commit `feat: add inline icons`.

### Task 4: Sidebar shell and pages

**Files:**
- Create: `crates/app/ui/style.css`, `crates/app/ui/app.js`, `crates/app/ui-test/chart.test.js`
- Modify: `crates/app/ui/index.html` (markup only; inline style and script move out), `crates/app/ui/table.js`, `crates/app/ui/i18n.js`

**Interfaces:**
- Consumes: Task 1 functions, `icon`, Task 2 data shapes, `makeTable`/`cell`.
- Produces: `PAGES = ["overview","apps","findings","startup","drives","settings","about"]`; `chart({title, icon, series: [{t, v}], color, max, fmt, note}): string`; script order `i18n.js, icons.js, table.js, app.js`; `app.js` starts only when `typeof document !== "undefined"`.

- [ ] **Step 1: Write the failing test**

```js
const test = require("node:test");
const assert = require("node:assert/strict");
globalThis.t = require("../ui/i18n.js").t;
globalThis.icon = () => "";
const { chart } = require("../ui/app.js");
const draw = (series, max = null) =>
  chart({ title: "CPU", icon: "cpu", series, color: "red", max, fmt: v => v + "u", note: "" });

test("empty and all-null series show a dash", () => {
  for (const s of [[], [{ t: 1, v: null }]]) {
    assert.ok(draw(s).includes("–"));
    assert.doesNotMatch(draw(s), /NaN|Infinity|undefined/);
  }
});
test("one sample and all-zero series stay finite", () => {
  assert.ok(draw([{ t: 1, v: 7 }]).includes("7u"));
  for (const s of [[{ t: 1, v: 7 }], [{ t: 1, v: 0 }, { t: 1, v: 0 }]])
    assert.doesNotMatch(draw(s), /NaN|Infinity|undefined/);
});
```

- [ ] **Step 2:** Run the JS tests. Expected: FAIL, cannot find `../ui/app.js`.
- [ ] **Step 3: Shell.** `index.html`: sidebar `<nav>` (Settings and About pinned at the bottom, update status under them), `<main>` with the banner and one `<section>` per page. Every page header is icon plus title; the single range `<select>` shows only on `overview`, `apps`, `findings`. Nav buttons carry `title`, `aria-label` and `aria-current="page"` on the selected one. Static text comes from `data-i18n` attributes filled with `t()`.
- [ ] **Step 4: Style.** `style.css` with color tokens including `--ok --warn --bad` and the five series colors, dark and light (light by `prefers-color-scheme` for now). Same centered content width on every page. Charts: `grid-template-columns: repeat(6, 1fr)`; first two cards span 3, other three span 2. At `max-width: 800px` the sidebar shows icons only and charts stack in one column. Wide tables scroll inside their card.
- [ ] **Step 5: Script.** `app.js` takes over the current inline script. `chart` shows current value, peak, value-axis maximum, and start and end time as HTML outside the SVG. Disk and Network titles are "Disk traffic" and "Network", with the admin/service requirement in `note`. Apps table and its "averages cover only samples where the app was active" note move to the Apps page. Findings cells use `tv("kind", …)`, `tv("sev", …)` (sort value stays the number) and `findingText`. Startup change cells use `tv("change", …)`. Labels are full words ("Severity", "Network", "Disk traffic"). All visible strings, including banner, status, service state and units wording, go through `t()`; every thrown command error goes through `errorText(String(e))`. The Settings page holds the existing collector card with the same behavior, loaded when the page opens.
- [ ] **Step 6: Table strings.** In `table.js` use `t()` with keys `table.search` "Search…", `table.columns` "Columns", `table.all` "{label}: all", `table.noMatches` "no matches", `table.empty` "nothing yet". Rename the local `const t = e.target` so it does not shadow `t`.
- [ ] **Step 7:** Run the JS tests. Expected: PASS.
- [ ] **Step 8:** Commit `feat: sidebar layout with separate pages`.

### Task 5: Theme, language, About

**Files:**
- Modify: `crates/app/ui/index.html`, `app.js`, `style.css`, `i18n.js`, `README.md`

**Interfaces:** Consumes `pickTheme`, `pickLang`, `setLang`, `STRINGS`, `service_status` (`version`, `db`). Storage keys `rw.theme`, `rw.lang`.

- [ ] **Step 1: Preferences.** In `app.js`, read both keys inside `try/catch`; call `setLang` before any table or text is built; set `document.documentElement.dataset.theme`. Writes are also in `try/catch`.
- [ ] **Step 2: Theme CSS.** `data-theme="light"|"dark"` overrides Windows; `system` follows `prefers-color-scheme`. Set `color-scheme` in each case so native controls match.
- [ ] **Step 3: Settings page.** Three cards, same width: Appearance (System, Light, Dark; applies immediately), Language (options from `STRINGS`, own name from key `lang.name`; change saves then `location.reload()`), Background collector.
- [ ] **Step 4: About page.** Name, version (`–` if status fails), description "Records per-app CPU, memory, disk, network and GPU history on Windows, flags spikes, and shows it in a local dashboard.", collector and window roles, data file path, license MIT, publisher zhmdff. No links.
- [ ] **Step 5: README.** Update the `app` row of the Layout table to the new pages and files, and "Settings tab" to "Settings page".
- [ ] **Step 6: Verify.** Run the JS tests and `cargo test`: PASS. Then `$env:RUSKWATCHER_DB="$PWD\ruskwatcher.db"; cargo run -p ruskwatcher-app` (with `cargo run -p service` collecting): each page in System, Light and Dark, at 1100x780 and 600x400; theme survives restart; table sort and hidden columns saved before the redesign still apply; with a nonexistent database path the banner shows the worded message.
- [ ] **Step 7:** Commit `feat: add theme, language and about page`.
