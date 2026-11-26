// Navigation, page rendering, charts and the collector settings.

const PAGES = ["overview", "apps", "findings", "startup", "drives", "settings", "about"];
const RANGE_PAGES = ["overview", "apps", "findings"];

const $ = s => document.querySelector(s);
const bytes = n => { if (n == null) return "–"; n = +n || 0; const u = t("unit.bytes").split(" "); let i = 0; while (n >= 1024 && i < u.length - 1) { n /= 1024; i++ } return n.toFixed(i ? 1 : 0) + " " + u[i] };
const pct = n => (n == null ? "–" : n.toFixed(1) + "%");
const when = ts => new Date(ts * 1000).toLocaleString();
const shortWhen = ts => new Date(ts * 1000).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
const dur = ms => (ms == null ? "–" : t("unit.seconds", { n: (ms / 1000).toFixed(1) }));
const invoke = (c, a) => window.__TAURI__.core.invoke(c, a);
// Storage can be unavailable; the app then runs on defaults.
const readPref = k => { try { return localStorage.getItem(k) } catch { return null } };
const writePref = (k, v) => { try { localStorage.setItem(k, v) } catch {} };

// Minimal SVG line chart: values already bucketed by the server. Titles and notes are our own strings.
function chart({ title, icon: name, series, color, max, fmt, note }) {
  const W = 400, H = 120, pts = series.filter(p => p.v != null);
  const peak = pts.length ? Math.max(...pts.map(p => p.v)) : null;
  const hi = max ?? Math.max(1, peak ?? 0);
  const t0 = pts[0]?.t, t1 = pts[pts.length - 1]?.t;
  const xy = p => `${((p.t - t0) / (t1 - t0 || 1) * W).toFixed(1)},${(H - 4 - p.v / hi * (H - 8)).toFixed(1)}`;
  let path = pts.map((p, i) => (i ? "L" : "M") + xy(p)).join("");
  if (pts.length === 1) path += "h0.01"; // a single sample shows as a dot
  const show = v => (v == null ? "–" : fmt(v));
  return `<div class="card chart" style="--c:${color}">
    <h2>${icon(name)}<span>${title}</span><b>${show(pts.length ? pts[pts.length - 1].v : null)}</b></h2>
    <div class="axis">${t("chart.max", { v: show(max ?? (pts.length ? hi : null)) })}</div>
    <svg class="plot" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" aria-hidden="true">
      <path d="${path}" fill="none" stroke="${color}" stroke-width="1.8" stroke-linecap="round" vector-effect="non-scaling-stroke"/>
    </svg>
    <div class="axis"><span>${pts.length ? shortWhen(t0) : ""}</span><span>${pts.length ? shortWhen(t1) : ""}</span></div>
    <div class="foot">${t("chart.peak", { v: show(peak) })}${note ? "<br>" + note : ""}</div></div>`;
}

function boot() {
  let page = "overview", svc = null;
  const lang = pickLang(readPref("rw.lang"));
  setLang(lang); // before any text or table is built
  document.documentElement.lang = lang;
  const theme = pickTheme(readPref("rw.theme"));
  document.documentElement.dataset.theme = theme;

  document.querySelectorAll("[data-i18n]").forEach(e => { e.textContent = t(e.dataset.i18n) });
  $("#range").setAttribute("aria-label", t("range.label"));
  $("#nav").setAttribute("aria-label", t("nav.label"));
  $("#nav").innerHTML = PAGES.map(p => {
    const name = t("nav." + p);
    return `<button type="button" data-page="${p}" title="${name}" aria-label="${name}">${icon(p)}<span class="label">${name}</span></button>`;
  }).join("");

  $("#theme").value = theme;
  $("#theme").onchange = e => {
    document.documentElement.dataset.theme = e.target.value;
    writePref("rw.theme", e.target.value);
  };
  $("#language").innerHTML = Object.keys(STRINGS).map(code => `<option value="${code}">${STRINGS[code]["lang.name"] ?? code}</option>`).join("");
  $("#language").value = lang;
  $("#language").onchange = e => { writePref("rw.lang", e.target.value); location.reload(); };

  function show() {
    PAGES.forEach(p => $("#" + p).classList.toggle("hidden", p !== page));
    document.querySelectorAll("#nav button").forEach(b => {
      if (b.dataset.page === page) b.setAttribute("aria-current", "page");
      else b.removeAttribute("aria-current");
    });
    $("#page-icon").innerHTML = icon(page);
    $("#page-title").textContent = t("nav." + page);
    $("#range").classList.toggle("hidden", !RANGE_PAGES.includes(page));
  }
  $("#nav").onclick = e => {
    const b = e.target.closest("button");
    if (!b) return;
    page = b.dataset.page;
    show();
    if (page === "settings" || page === "about") loadService();
  };

  const col = (id, label, extra) => ({ id, label: t("col." + label), ...extra });
  const num = { num: true }, filter = { filter: true };
  const tables = {
    apps: makeTable($("#apps-table"), "apps", [col("app", "app"), col("signed", "signed", filter), col("cpu_avg", "cpuAvg", num), col("cpu_max", "cpuMax", num),
      col("mem", "memMax", num), col("gpu", "gpuAvg", num), col("disk", "disk", num), col("net", "network", num)]),
    findings: makeTable($("#ftable"), "findings", [col("when", "when"), col("kind", "kind", filter), col("sev", "severity", filter),
      col("app", "app"), col("summary", "summary")]),
    startup: makeTable($("#stable"), "startup", [col("name", "name"), col("source", "source", filter), col("state", "state", filter), col("command", "command")]),
    changes: makeTable($("#ctable"), "changes", [col("when", "when"), col("change", "change", filter), col("name", "name"), col("detail", "detail")]),
    boots: makeTable($("#btable"), "boots", [col("when", "bootStarted"), col("total", "total", num), col("main", "mainPath", num), col("post", "postBoot", num)]),
    drives: makeTable($("#dtable"), "drives", [col("model", "model"), col("type", "type", filter), col("size", "size", num), col("temp", "temperature", num),
      col("wear", "wear", num), col("spare", "spare", num), col("written", "written", num), col("hours", "powerOn", num), col("errors", "mediaErrors", num)]),
  };
  // display text from fmt, sorting by the raw value
  const val = (v, fmt) => cell(v == null ? "–" : fmt(v), v);
  const at = ts => cell(when(ts), ts);
  const tag = (key, cls) => cell(t(key), t(key), cls);

  function banner(msg) {
    $("#banner").classList.toggle("hidden", !msg);
    // rewriting the same text makes screen readers repeat it on every refresh
    if ($("#banner").textContent !== msg) $("#banner").textContent = msg;
  }

  function render(d) {
    const memTotal = d.mem_total || 1;
    const draw = (name, color, value, max, fmt, note) =>
      chart({ title: t("chart." + name), icon: name, color, max, fmt, note, series: d.series.map(p => ({ t: p.ts, v: value(p) })) });
    const rate = v => t("unit.perSecond", { n: bytes(v) });
    const admin = t("chart.needsAdmin");
    $("#charts").innerHTML =
      draw("cpu", "var(--cpu)", p => p.cpu, 100, pct) +
      draw("memory", "var(--mem)", p => (p.mem == null ? null : p.mem / memTotal * 100), 100, pct) +
      draw("gpu", "var(--gpu)", p => p.gpu, 100, pct) +
      draw("disk", "var(--disk)", p => p.disk_bps, null, rate, admin) +
      draw("network", "var(--net)", p => p.net_bps, null, rate, admin);

    tables.apps.set(d.apps.map(a => [cell(a.name),
      a.signed === 1 ? tag("value.yes", "ok") : a.signed === 0 ? tag("value.no", "warn") : tag("value.unknown", "dim"),
      val(a.cpu_avg, pct), val(a.cpu_max, pct), val(a.mem_max, bytes), val(a.gpu_avg, pct), val(a.disk_bytes, bytes), val(a.net_bytes, bytes)]));

    tables.findings.set(d.findings.map(f => [at(f.ts), cell(tv("kind", f.kind)),
      cell(tv("sev", f.severity), f.severity, f.severity > 1 ? "bad" : "warn"),
      cell(f.app || "–", f.app || null), cell(findingText(f))]));

    tables.startup.set(d.startup.map(e => [cell(e.name), cell(tv("source", e.source)),
      e.enabled ? tag("value.enabled", "ok") : tag("value.disabled", "dim"), cell(e.command)]));
    tables.changes.set(d.startup_changes.map(c => [at(c.ts), cell(tv("change", c.change)), cell(c.name), cell(c.new || c.old || "")]));
    tables.boots.set(d.boots.map(b => [at(b.ts), val(b.duration_ms, dur), val(b.main_path_ms, dur), val(b.post_boot_ms, dur)]));

    tables.drives.set(d.drives.map(x => [cell(x.model), cell(tv("drive", x.kind)), val(x.capacity, bytes),
      val(x.temp_c, n => t("unit.celsius", { n: n.toFixed(0) })), val(x.percent_used, pct), val(x.spare, pct),
      val(x.host_writes, bytes), val(x.power_on_hours, n => t("unit.hours", { n })), val(x.media_errors, String)]));

    const stale = !d.last_sample || d.now - d.last_sample > 30;
    banner(stale ? t("banner.stale", { time: d.last_sample ? when(d.last_sample) : t("banner.never") })
      : !d.elevated ? t("banner.notElevated") : "");
  }

  async function load() {
    try {
      render(await invoke("dashboard", { range: +$("#range").value }));
      $("#status").textContent = t("status.updated", { time: new Date().toLocaleTimeString() });
    } catch (e) {
      $("#status").textContent = "";
      banner(errorText(e));
    }
  }

  async function loadService() {
    try {
      svc = await invoke("service_status");
    } catch (e) {
      $("#svc-msg").textContent = errorText(e);
      return;
    }
    $("#svc-state").textContent = t(!svc.installed ? "svc.notInstalled" : svc.running ? "svc.running" : "svc.stopped");
    $("#svc-state").className = !svc.installed ? "bad" : svc.running ? "ok" : "warn";
    $("#svc-auto").checked = svc.autostart;
    $("#svc-auto").disabled = !svc.installed;
    $("#svc-run").textContent = t(svc.running ? "svc.stop" : "svc.start");
    $("#svc-run").disabled = !svc.installed;
    $("#about-version").textContent = svc.version;
    document.querySelectorAll(".db-path").forEach(e => { e.textContent = svc.db });
  }
  async function elevated(fn, args) {
    $("#svc-msg").textContent = t("svc.waiting");
    try { await invoke(fn, args); $("#svc-msg").textContent = t("svc.done"); }
    catch (e) { $("#svc-msg").textContent = t("svc.notChanged", { error: errorText(e) }); }
    await loadService();
  }
  $("#svc-auto").onchange = e => elevated("set_autostart", { on: e.target.checked });
  $("#svc-run").onclick = () => elevated("set_running", { on: !svc.running });
  $("#range").onchange = load;

  show(); load(); setInterval(load, 5000);
}

if (typeof document !== "undefined") boot();
if (typeof module !== "undefined") module.exports = { PAGES, chart };
