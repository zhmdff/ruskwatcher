// All user-facing wording. A new language is one more entry in STRINGS; missing keys fall back to English.

const STRINGS = {
  en: {
    "nav.overview": "Overview",
    "nav.apps": "Apps",
    "nav.findings": "Findings",
    "nav.startup": "Startup",
    "nav.drives": "Drives",
    "nav.settings": "Settings",
    "nav.about": "About",

    "kind.system_cpu_high": "High CPU (system)",
    "kind.app_cpu_high": "High CPU (app)",
    "kind.memory_pressure": "Memory pressure",
    "kind.disk_heavy": "Heavy disk traffic",
    "change.added": "Added",
    "change.changed": "Changed",
    "change.removed": "Removed",
    "drive.nvme": "NVMe",
    "drive.ssd": "SSD",
    "drive.hdd": "HDD",
    "source.run": "Registry Run key",
    "source.folder": "Startup folder",
    "sev.1": "Warning",
    "sev.2": "High",

    "finding.system_cpu_high": "CPU averaged {v}% over 20 s; busiest app {app}",
    "finding.system_cpu_high.noValue": "CPU stayed high over 20 s; busiest app {app}",
    "finding.app_cpu_high": "{app} averaged {v}% CPU over 20 s",
    "finding.app_cpu_high.noValue": "{app} used a lot of CPU over 20 s",
    "finding.memory_pressure": "RAM {v}% used; largest app {app}",
    "finding.memory_pressure.noValue": "RAM was nearly full over 10 s; largest app {app}",
    "finding.disk_heavy": "{app} moved {v} MB/s over 10 s",
    "finding.disk_heavy.noValue": "{app} moved a lot of disk data over 10 s",
    "finding.unknownApp": "unknown",

    "error.no_data": "No data at {path} yet. Is the RuskWatcher service running?",
    "error.service_exe_missing": "Service program not found: {path}",
    "error.elevation_failed": "cancelled or failed",

    "nav.label": "Pages",
    "range.label": "Time range",
    "range.900": "15 minutes",
    "range.3600": "1 hour",
    "range.21600": "6 hours",
    "range.86400": "24 hours",
    "range.604800": "7 days",
    "range.2592000": "30 days",
    "status.loading": "Loading…",
    "status.updated": "Updated {time}",
    "banner.stale": "The RuskWatcher service is not collecting right now (last sample: {time}).",
    "banner.never": "never",
    "banner.notElevated": "Collector is not elevated: per-app disk and network are off.",

    "chart.cpu": "CPU",
    "chart.memory": "Memory",
    "chart.gpu": "GPU",
    "chart.disk": "Disk traffic",
    "chart.network": "Network",
    "chart.max": "Max {v}",
    "chart.peak": "Peak {v}",
    "chart.needsAdmin": "Needs admin or the service",
    "unit.bytes": "B KB MB GB TB",
    "unit.perSecond": "{n}/s",
    "unit.seconds": "{n} s",
    "unit.hours": "{n} h",
    "unit.celsius": "{n} °C",

    "title.apps": "Apps in range",
    "note.apps": "Averages cover only samples where the app was active.",
    "title.findings": "Findings",
    "title.startup": "Startup entries",
    "title.changes": "Recent startup changes",
    "title.boots": "Boots",
    "title.drives": "Drives",

    "col.app": "App",
    "col.signed": "Signed",
    "col.cpuAvg": "CPU average",
    "col.cpuMax": "CPU peak",
    "col.memMax": "Memory peak",
    "col.gpuAvg": "GPU average",
    "col.disk": "Disk traffic",
    "col.network": "Network",
    "col.when": "When",
    "col.kind": "Kind",
    "col.severity": "Severity",
    "col.summary": "What happened",
    "col.name": "Name",
    "col.source": "Source",
    "col.state": "State",
    "col.command": "Command",
    "col.change": "Change",
    "col.detail": "Detail",
    "col.bootStarted": "Boot started",
    "col.total": "Total time",
    "col.mainPath": "Main path",
    "col.postBoot": "After boot",
    "col.model": "Model",
    "col.type": "Type",
    "col.size": "Size",
    "col.temperature": "Temperature",
    "col.wear": "Wear used",
    "col.spare": "Spare capacity",
    "col.written": "Data written",
    "col.powerOn": "Power-on time",
    "col.mediaErrors": "Media errors",
    "value.yes": "Yes",
    "value.no": "No",
    "value.unknown": "Unknown",
    "value.enabled": "Enabled",
    "value.disabled": "Disabled",

    "table.search": "Search…",
    "table.searchLabel": "Search",
    "table.columns": "Columns",
    "table.all": "{label}: all",
    "table.noMatches": "No matches",
    "table.empty": "Nothing yet",

    "svc.title": "Background collector",
    "svc.intro": "RuskWatcher records data with a small Windows service. This window only reads what it saved, so you can close it any time and collection continues.",
    "svc.status": "Status:",
    "svc.notInstalled": "Not installed",
    "svc.running": "Running",
    "svc.stopped": "Stopped",
    "svc.auto": "Start collecting automatically when Windows starts",
    "svc.start": "Start collecting",
    "svc.stop": "Stop collecting",
    "svc.waiting": "Waiting for administrator approval…",
    "svc.done": "Done.",
    "svc.notChanged": "Not changed ({error}).",
    "svc.adminHint": "Changing these asks Windows for administrator approval.",
    "svc.dataFile": "Data file:",

    "lang.name": "English",
    "settings.appearance": "Appearance",
    "settings.theme": "Theme",
    "settings.theme.system": "System (follow Windows)",
    "settings.theme.light": "Light",
    "settings.theme.dark": "Dark",
    "settings.language": "Language",

    "about.description": "Records per-app CPU, memory, disk, network and GPU history on Windows, flags spikes, and shows it in a local dashboard.",
    "about.version": "Version",
    "about.collector": "Collector service",
    "about.collectorRole": "Samples in the background, saves history and raises findings. It has no window and no network port.",
    "about.window": "Dashboard window",
    "about.windowRole": "Only reads what the service saved. Closing it does not stop collection.",
    "about.dataFile": "Data file",
    "about.license": "License",
    "about.publisher": "Publisher",
  },
};

let lang = "en";
const known = (o, k) => Object.hasOwn(o, k);
const pickLang = raw => (known(STRINGS, raw) ? raw : "en");
const pickTheme = raw => (raw === "light" || raw === "dark" ? raw : "system");
const setLang = raw => { lang = pickLang(raw) };
const lookup = key => known(STRINGS[lang], key) ? STRINGS[lang][key] : known(STRINGS.en, key) ? STRINGS.en[key] : null;

// One pass with a function replacer: values containing {x} or $& stay literal.
function t(key, vars = {}) {
  return (lookup(key) ?? key).replace(/\{(\w+)\}/g, (m, name) => (known(vars, name) ? String(vars[name]) : m));
}

// Wording for a value from the database; unrecognised values show raw.
const tv = (group, value) => lookup(`${group}.${value}`) ?? String(value);

function findingText({ kind, value, app }) {
  const key = "finding." + kind;
  if (lookup(key) == null) return String(kind);
  const vars = { app: app || t("finding.unknownApp") };
  if (value == null) return t(key + ".noValue", vars);
  vars.v = (kind === "disk_heavy" ? value / 1048576 : value).toFixed(0);
  return t(key, vars);
}

// Backend errors are "code" or "code:path"; anything else is shown as is.
function errorText(error) {
  const raw = String(error), i = raw.indexOf(":");
  const key = "error." + (i < 0 ? raw : raw.slice(0, i));
  return lookup(key) == null ? raw : t(key, { path: i < 0 ? "" : raw.slice(i + 1) });
}

if (typeof module !== "undefined") module.exports = { STRINGS, pickLang, pickTheme, setLang, t, tv, findingText, errorText };
