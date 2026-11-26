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
