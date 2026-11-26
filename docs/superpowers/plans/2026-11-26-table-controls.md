# Dashboard Table Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every dashboard table gets search, click-header sort, dropdown filters and a saved show/hide columns menu.

**Architecture:** The `table()` function in `index.html` is replaced by `ui/table.js`: pure view logic (testable in Node) plus a small DOM component that builds its controls once and redraws rows on refresh. The store query returns more rows so sorting is not limited to a truncated subset.

**Tech Stack:** Plain HTML/CSS/JS (Tauri webview), Rust + rusqlite, `node --test`.

**Spec:** `docs/superpowers/specs/2026-11-26-table-controls-design.md`

## Global Constraints

- Plain HTML, CSS and JS in `crates/app/ui`; no build step, no dependency, no CDN.
- The app stays read-only toward the database.
- Data values are escaped before insertion; saved settings and search text are never inserted as HTML.
- Light and dark themes both work (use the existing CSS variables); controls are keyboard usable; sorted headers set `aria-sort`.
- The 5 s refresh must not clear search text, steal focus, close an open menu, or reset sort and filters.
- Row limits: apps unlimited, findings 500, startup changes 200, boots 50.
- Commit steps run only if the user has asked for commits; if so, branch off `main` first.

## Review Focus

- Search text or data containing `<`, `&`, `"`: matched literally, shown literally (Task 2).
- Value `0` versus missing: `0` sorts as a number, only missing goes last (Task 2).
- Corrupt or stale saved settings, including a hidden first column: defaults, first column visible (Task 2).
- Selected filter value gone after a refresh: stays selected and listed, table says "no matches" (Task 2).
- Refresh while a header button or dropdown has focus: focus and open dropdown survive (Task 3).

---

### Task 1: Row limits

**Files:**
- Modify: `crates/store/src/query.rs` (queries at lines 54, 79, 146, 162; tests module at the bottom)

**Interfaces:**
- Produces: `Store::dashboard(range, now)` with the same JSON shape; only row counts change.

- [ ] **Step 1: Write the failing test** `dashboard_row_limits` in the `tests` module

Seed through `s.conn.execute_batch` on an in-memory store with `now = 10_000`: 35 `process` rows with one tier-0 `proc_sample` each at `ts = 9_000`; 60 `finding` rows at `ts_start = 9_000` (`detail_json = '{}'`); one `startup_entry` with 25 `startup_change` rows; 7 `boot` rows. A recursive CTE keeps the seeding short. Assert on `s.dashboard(3600, 10_000)`:

```rust
let len = |k: &str| v[k].as_array().unwrap().len();
assert_eq!(len("apps"), 35);
assert_eq!(len("findings"), 60);
assert_eq!(len("startup_changes"), 25);
assert_eq!(len("boots"), 7);
```

- [ ] **Step 2:** Run `cargo test -p store dashboard_row_limits`. Expected: FAIL, `left: 30, right: 35`.
- [ ] **Step 3:** Remove `LIMIT 30` from the apps query; set findings to `LIMIT 500`, startup changes to `LIMIT 200`, boots to `LIMIT 50`. Leave `ORDER BY` unchanged.
- [ ] **Step 4:** Run `cargo test -p store`, `cargo fmt --check`, `cargo clippy -p store`. Expected: all pass, no new warnings.
- [ ] **Step 5:** Commit: `feat: return more rows for dashboard tables`

### Task 2: Table view logic

**Files:**
- Create: `crates/app/ui/table.js`
- Test: `crates/app/ui-test/table.test.js` (outside `ui/`, so not bundled)

**Interfaces:**
- Produces (globals in the browser, `module.exports` under Node when `typeof module !== "undefined"`):
  - `cell(text, v = text, cls = "") -> {text: string, v: number|string|null, cls: string}`; `text` is plain text, never HTML. `v` of `null`, `undefined` or `""` means missing.
  - Column: `{id: string, label: string, num?: true, filter?: true}`. A row is `Cell[]` in column order.
  - State: `{q: string, filters: {[colId]: string}, sort: {id: string, dir: 1|-1}|null, hidden: string[]}`. A filter value of `""` or absent means "all"; filters compare `cell.text` exactly.
  - `view(rows, cols, state) -> rows`: new array, input untouched, stable.
  - `nextSort(sort, id) -> sort`: ascending, descending, `null`; another column starts ascending.
  - `options(rows, colIndex, selected) -> string[]`: distinct texts plus `selected` if non-empty, locale sorted.
  - `loadPrefs(raw: string|null, cols) -> {sort, hidden}`: never throws.

- [ ] **Step 1: Write the failing tests** (`node:test`, `node:assert/strict`)

```js
const cols = [{ id: "name", label: "Name" }, { id: "mem", label: "Mem", num: true }];
const rows = [
  [cell("b<&\"", "b<&\""), cell("2 GB", 2e9)],
  [cell("A"), cell("900 MB", 9e8)],
  [cell("c"), cell("–", null)],
  [cell("d"), cell("0 B", 0)],
];
const st = o => ({ q: "", filters: {}, sort: null, hidden: [], ...o });
const names = r => r.map(x => x[0].text);

test("sorts by raw value, zero is a value, missing last both ways", () => {
  assert.deepEqual(names(view(rows, cols, st({ sort: { id: "mem", dir: 1 } }))), ["d", "A", "b<&\"", "c"]);
  assert.deepEqual(names(view(rows, cols, st({ sort: { id: "mem", dir: -1 } }))), ["b<&\"", "A", "d", "c"]);
  assert.deepEqual(names(rows), ["b<&\"", "A", "c", "d"]);
});
test("text sorts by locale", () =>
  assert.deepEqual(names(view(rows, cols, st({ sort: { id: "name", dir: 1 } }))), ["A", "b<&\"", "c", "d"]));
test("search is literal, case-insensitive, includes hidden columns", () => {
  assert.deepEqual(names(view(rows, cols, st({ q: "B<&" }))), ["b<&\""]);
  assert.deepEqual(names(view(rows, cols, st({ q: "900 mb", hidden: ["mem"] }))), ["A"]);
});
test("search and filters combine", () => {
  assert.deepEqual(names(view(rows, cols, st({ filters: { name: "A" } }))), ["A"]);
  assert.deepEqual(view(rows, cols, st({ q: "gb", filters: { name: "A" } })), []);
});
test("sort cycles", () => {
  const a = nextSort(null, "mem"), d = nextSort(a, "mem");
  assert.deepEqual([a, d, nextSort(d, "mem")], [{ id: "mem", dir: 1 }, { id: "mem", dir: -1 }, null]);
  assert.deepEqual(nextSort(d, "name"), { id: "name", dir: 1 });
});
test("selected filter value stays listed", () => {
  assert.deepEqual(options(rows, 0, "gone"), ["A", "b<&\"", "c", "d", "gone"]);
  assert.deepEqual(view(rows, cols, st({ filters: { name: "gone" } })), []);
});
test("bad saved prefs fall back", () => {
  const def = { sort: null, hidden: [] };
  for (const raw of [null, "{", "null", "7", '{"hidden":"mem","sort":{"id":"x","dir":1}}', '{"sort":{"id":"mem","dir":5}}'])
    assert.deepEqual(loadPrefs(raw, cols), def);
  assert.deepEqual(loadPrefs('{"hidden":["name","x","mem"],"sort":{"id":"mem","dir":-1}}', cols),
    { sort: { id: "mem", dir: -1 }, hidden: ["mem"] });
});
```

- [ ] **Step 2:** Run `node --test crates/app/ui-test/table.test.js`. Expected: FAIL, cannot find `../ui/table.js`.
- [ ] **Step 3:** Implement the five functions in `table.js`. Compare numbers by subtraction and strings with `localeCompare`; apply the missing-last rule before multiplying by `dir`.
- [ ] **Step 4:** Rerun the command. Expected: all pass.
- [ ] **Step 5:** Commit: `feat: add table search, filter and sort logic`

### Task 3: Table controls in the dashboard

**Files:**
- Modify: `crates/app/ui/table.js`
- Modify: `crates/app/ui/index.html` (CSS lines 25-28, heading line 56, helpers and `table()` lines 83 and 117-121, `render()` lines 132-149)

**Interfaces:**
- Consumes: everything produced by Task 2.
- Produces: `makeTable(el: HTMLTableElement, key: string, cols) -> {set(rows: Cell[][]): void}`.

- [ ] **Step 1:** Implement `makeTable`:
  - Built once: a toolbar inserted before `el` with a search input (`aria-label="Search"`), one `<select>` per `filter` column (first option `<label>: all`, `aria-label` = label), and a `<details>` menu "Columns" with a checkbox per column; the first column's checkbox is disabled and checked.
  - `<thead>`: each header is a `<button>` with ▲/▼ for the active sort and `aria-sort` `ascending`/`descending` on the `th`. Redrawn only on sort or visibility change, then focus returns to the clicked button.
  - `<tbody>`: redrawn on every `set` and state change. Each `td` has `title` and content escaped from `cell.text`, wrapped in `<span class="cls">` when `cls` is set. Empty: "nothing yet" if no rows, "no matches" if the view is empty; `colspan` = visible column count.
  - Filter `<option>`s are rewritten only when the `options()` list changes, so an open dropdown survives refresh.
  - `localStorage` key `rw.table.<key>` stores `{sort, hidden}`; wrap reads and writes in `try`.
  - Move `esc` from `index.html` into `table.js`.
- [ ] **Step 2:** In `index.html`, load `<script src="table.js"></script>` before the inline script, delete `table()`, and create the six tables once (keys `apps`, `findings`, `startup`, `changes`, `boots`, `drives`; elements `#apps`, `#ftable`, `#stable`, `#ctable`, `#btable`, `#dtable`), keeping current labels and alignment. `render()` calls `.set()` with `cell(...)` values:
  - Raw `v` is the source number for percentages, bytes, durations, timestamps, severity, temperature, hours and error counts; `null` source means `v = null`. Display text is unchanged, except power-on shows `–` (not `– h`) when missing.
  - Signed: `yes` (`ok`), `no` (`warn`), `?` (`dim`, `v` stays `"?"` so it can be filtered).
  - Findings app with no value: text `–`, `v = null`. Severity keeps `bad`/`warn`; enabled/disabled keeps `ok`/`dim`.
  - Filter columns: apps Signed; findings Kind, Sev; startup Source, State; changes Change; drives Type.
- [ ] **Step 3:** Rename the heading "Top apps in range" to "Apps in range". Add CSS for the toolbar, menu panel and header buttons using the existing variables; style `input` like `select`.
- [ ] **Step 4:** Run `node --test crates/app/ui-test/table.test.js` and `cargo test`. Expected: all pass.
- [ ] **Step 5:** Run `cargo run -p ruskwatcher-app` (reads `C:\ProgramData\RuskWatcher\ruskwatcher.db`). Check on each tab: search, each filter, sort cycle, hide and show a column; wait past two refreshes with search focused, a header button focused, the Columns menu open and a dropdown open; restart and confirm sort and hidden columns return while search and filters are empty; check light and dark themes.
- [ ] **Step 6:** Commit: `feat: add search, filters, sorting and column toggles to dashboard tables`
