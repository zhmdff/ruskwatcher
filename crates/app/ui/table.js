// Dashboard tables: search, filters, sort and column visibility.

// text is what the user sees (plain, never HTML); v is what sorting compares.
function cell(text, v = text, cls = "") {
  return { text: String(text ?? ""), v, cls };
}

const missing = v => v == null || v === "";

function view(rows, cols, state) {
  const q = state.q.toLowerCase();
  const filters = cols.map((c, i) => [i, state.filters[c.id]]).filter(f => f[1]);
  const out = rows.filter(r =>
    filters.every(([i, want]) => r[i].text === want) &&
    (!q || r.some(c => c.text.toLowerCase().includes(q))));
  const i = cols.findIndex(c => c.id === state.sort?.id);
  if (i < 0 || state.hidden.includes(cols[i].id)) return out; // never order rows by something the user cannot see
  return out.sort((a, b) => {
    const x = a[i].v, y = b[i].v;
    if (missing(x) || missing(y)) return missing(x) - missing(y);
    return state.sort.dir * (typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y)));
  });
}

function nextSort(sort, id) {
  if (sort?.id !== id) return { id, dir: 1 };
  return sort.dir === 1 ? { id, dir: -1 } : null;
}

function options(rows, colIndex, selected) {
  const set = new Set(rows.map(r => r[colIndex].text));
  if (selected) set.add(selected);
  return [...set].sort((a, b) => a.localeCompare(b));
}

// Saved settings may be corrupt or name columns that no longer exist. The first column is never hidden.
function loadPrefs(raw, cols) {
  let p = null;
  try { p = JSON.parse(raw); } catch {}
  const ids = cols.map(c => c.id);
  const hidden = Array.isArray(p?.hidden) ? ids.slice(1).filter(id => p.hidden.includes(id)) : [];
  const s = p?.sort;
  const sort = s && ids.includes(s.id) && (s.dir === 1 || s.dir === -1) ? { id: s.id, dir: s.dir } : null;
  return { sort, hidden };
}

const esc = s => String(s ?? "").replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));

// Controls are built once; refreshes redraw only the rows, so focus and open menus survive.
function makeTable(el, key, cols) {
  const slot = "rw.table." + key;
  let saved = null;
  try { saved = localStorage.getItem(slot); } catch {}
  const state = { q: "", filters: {}, ...loadPrefs(saved, cols) };
  const save = () => {
    try { localStorage.setItem(slot, JSON.stringify({ sort: state.sort, hidden: state.hidden })); } catch {}
  };
  let rows = [];

  const bar = document.createElement("div");
  bar.className = "tools";
  bar.innerHTML = `<input type="search" placeholder="${esc(t("table.search"))}" aria-label="${esc(t("table.searchLabel"))}">` +
    cols.map((c, i) => c.filter ? `<select data-i="${i}" aria-label="${esc(c.label)}"></select>` : "").join("") +
    `<details><summary>${esc(t("table.columns"))}</summary><div>${cols.map((c, i) =>
      `<label><input type="checkbox" value="${c.id}" ${state.hidden.includes(c.id) ? "" : "checked"} ${i ? "" : "disabled"}> ${esc(c.label)}</label>`).join("")}</div></details>`;
  const scroll = document.createElement("div"); // wide tables scroll inside their card
  scroll.className = "scroll";
  el.before(bar, scroll);
  scroll.append(el);
  el.innerHTML = "<thead></thead><tbody></tbody>";
  const [head, body] = el.children;

  const shown = () => cols.map((_, i) => i).filter(i => !state.hidden.includes(cols[i].id));
  const align = i => cols[i].num ? "n" : "";

  function drawHead() {
    head.innerHTML = `<tr>${shown().map(i => {
      const c = cols[i], dir = state.sort?.id === c.id ? state.sort.dir : 0;
      const aria = dir ? ` aria-sort="${dir > 0 ? "ascending" : "descending"}"` : "";
      return `<th class="${align(i)}"${aria}><button type="button" data-id="${c.id}">${esc(c.label)}${dir ? (dir > 0 ? " ▲" : " ▼") : ""}</button></th>`;
    }).join("")}</tr>`;
  }

  function drawBody() {
    const on = shown(), list = view(rows, cols, state);
    body.innerHTML = list.length
      ? list.map(r => `<tr>${on.map(i => {
          const c = r[i], text = esc(c.text);
          return `<td class="${align(i)}" title="${text}">${c.cls ? `<span class="${c.cls}">${text}</span>` : text}</td>`;
        }).join("")}</tr>`).join("")
      : `<tr><td class="dim" colspan="${on.length}">${esc(t(rows.length ? "table.noMatches" : "table.empty"))}</td></tr>`;
  }

  // Rewritten only when the choices change, so an open dropdown is left alone by the refresh.
  function drawFilters() {
    bar.querySelectorAll("select").forEach(s => {
      const c = cols[s.dataset.i], selected = state.filters[c.id] || "";
      const html = `<option value="">${esc(t("table.all", { label: c.label }))}</option>` +
        options(rows, +s.dataset.i, selected).map(o => `<option value="${esc(o)}">${esc(o)}</option>`).join("");
      if (s.dataset.html === html) return;
      s.dataset.html = s.innerHTML = html;
      s.value = selected;
    });
  }

  bar.addEventListener("input", e => {
    const el = e.target;
    if (el.type === "search") state.q = el.value;
    else if (el.dataset.i != null) state.filters[cols[el.dataset.i].id] = el.value;
    else {
      state.hidden = [...bar.querySelectorAll("input[type=checkbox]")].filter(b => !b.checked).map(b => b.value);
      save();
      drawHead();
    }
    drawBody();
  });
  head.addEventListener("click", e => {
    const id = e.target.closest("button")?.dataset.id;
    if (!id) return;
    state.sort = nextSort(state.sort, id);
    save();
    drawHead();
    head.querySelector(`button[data-id="${id}"]`).focus();
    drawBody();
  });

  drawFilters(); drawHead(); drawBody();
  return { set(next) { rows = next; drawFilters(); drawBody(); } };
}

if (typeof module !== "undefined")module.exports = { cell, view, nextSort, options, loadPrefs };
