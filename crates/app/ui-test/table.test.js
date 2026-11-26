// Run: node --test crates/app/ui-test/table.test.js
const test = require("node:test");
const assert = require("node:assert/strict");
const { cell, view, nextSort, options, loadPrefs } = require("../ui/table.js");

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
test("a hidden column does not sort", () =>
  assert.deepEqual(names(view(rows, cols, st({ sort: { id: "mem", dir: 1 }, hidden: ["mem"] }))), names(rows)));
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
