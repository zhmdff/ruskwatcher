// Inline SVG icons on a 24x24 grid, drawn in the current text color.

const ICON_PATHS = {
  overview: "M3 3h7v7H3zM14 3h7v7h-7zM14 14h7v7h-7zM3 14h7v7H3z",
  apps: "M3 5h18v14H3zM3 9h18M6 7h.01M9 7h.01",
  findings: "M12 3 2 20h20zM12 10v4M12 17h.01",
  startup: "M12 3v9M7 6a8 8 0 1 0 10 0",
  drives: "M3 13h18v7H3zM3 13l3-9h12l3 9M7 16.5h.01M11 16.5h.01",
  settings: "M4 6h10M18 6h2M14 4v4h4V4zM4 12h2M10 12h10M6 10v4h4v-4zM4 18h10M18 18h2M14 16v4h4v-4z",
  about: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M12 11v6M12 7.5h.01",
  cpu: "M6 6h12v12H6zM10 10h4v4h-4zM9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4",
  memory: "M3 7h18v10H3zM7 10v4M12 10v4M17 10v4M6 17v3M12 17v3M18 17v3",
  gpu: "M2 6h20v11H2zM9 8.5a3 3 0 1 0 0 6 3 3 0 0 0 0-6M15 10h4M15 13h4M5 17v3",
  disk: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M12 10a2 2 0 1 0 0 4 2 2 0 0 0 0-4",
  network: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18",
};

const icon = name => Object.hasOwn(ICON_PATHS, name)
  ? `<svg class="icon" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="${ICON_PATHS[name]}"/></svg>`
  : "";

if (typeof module !== "undefined") module.exports = { icon };
