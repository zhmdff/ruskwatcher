// Run: node --test "crates/app/ui-test/*.test.js"
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
test("drive types and startup sources are worded", () => {
  assert.deepEqual(["nvme", "ssd", "hdd", "tape"].map(v => tv("drive", v)), ["NVMe", "SSD", "HDD", "tape"]);
  assert.deepEqual(["run", "folder", "task"].map(v => tv("source", v)), ["Registry Run key", "Startup folder", "task"]);
});
test("errors", () => {
  assert.ok(errorText("no_data:C:\\d\\r.db").includes("C:\\d\\r.db"));
  assert.ok(errorText("service_exe_missing:C:\\a\\service.exe").includes("C:\\a\\service.exe"));
  assert.notEqual(errorText("elevation_failed"), "elevation_failed");
  assert.equal(errorText("database is locked"), "database is locked");
});
