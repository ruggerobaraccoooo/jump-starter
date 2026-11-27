import { invoke, listen, formatShortcut, defaultShortcuts, iconFor, el, svg, isMac, platform } from "./common.js";

let groups = [];
let selectedId = null;
let failed = [];
let running = new Map();
let saveTimer = null;
let lastLocalSave = 0;

const list = document.getElementById("list");
const detail = document.getElementById("detail");

document.getElementById("bar").append(
  el("button", { class: "circle filled sm", title: "New group", onclick: addGroup }, svg("plus")),
  el("button", { class: "circle sm", title: "Delete group", onclick: removeGroup }, svg("minus")),
);
document.addEventListener("contextmenu", (e) => { if (!e.target.closest("input")) e.preventDefault(); });

const selected = () => groups.find((g) => g.id === selectedId);

async function load() {
  groups = await invoke("get_groups");
  failed = await invoke("failed_shortcuts");
  running = new Map((await invoke("running_apps")).map((r) => [r.path, r]));
  if (!groups.some((g) => g.id === selectedId)) selectedId = groups[0]?.id ?? null;
  renderList();
  renderDetail();
}

/** Save shortly after the last change; re-registers the global shortcuts. */
function scheduleSave(rerender = false) {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    saveTimer = null;
    lastLocalSave = Date.now();
    failed = await invoke("save_groups", { groups });
    if (rerender) renderDetail(); else updateErrors();
  }, 250);
}

// MARK: Sidebar

function renderList() {
  list.replaceChildren(...groups.map((g, i) => {
    const row = el("div", { class: "row" + (g.id === selectedId ? " selected on-accent" : ""), draggable: "true" },
      el("span", { class: "num" }, String(i + 1).padStart(2, "0")),
      el("span", { class: "name" }, g.name),
      g.shortcuts.open ? el("span", { class: "pill" + (g.id === selectedId ? " active" : "") }, formatShortcut(g.shortcuts.open)) : null);
    row.onclick = () => { selectedId = g.id; renderList(); renderDetail(); };
    // Drag to reorder
    row.ondragstart = (e) => e.dataTransfer.setData("text/plain", String(i));
    row.ondragover = (e) => { e.preventDefault(); row.classList.add("drop-target"); };
    row.ondragleave = () => row.classList.remove("drop-target");
    row.ondrop = (e) => {
      e.preventDefault();
      const from = Number(e.dataTransfer.getData("text/plain"));
      const [moved] = groups.splice(from, 1);
      groups.splice(i, 0, moved);
      renderList();
      scheduleSave();
    };
    return row;
  }));
}

// MARK: Detail

function renderDetail() {
  const g = selected();
  if (!g) {
    detail.replaceChildren(el("div", { class: "placeholder" },
      el("h2", {}, "Start your first group"),
      el("p", { class: "muted" }, "Collect apps you use together, then open them with one shortcut."),
      el("button", { class: "circle filled", onclick: addGroup }, svg("plus"))));
    return;
  }
  const index = groups.indexOf(g) + 1;
  const name = el("input", { class: "name-input", value: g.name, spellcheck: "false" });
  name.oninput = () => { g.name = name.value; renderList(); scheduleSave(); };

  const open = g.apps.filter((a) => running.has(a.path)).length;
  detail.replaceChildren(
    el("div", { class: "crumb muted" }, `Group ${String(index).padStart(2, "0")} / ${String(groups.length).padStart(2, "0")}`),
    name,
    el("div", { class: "shortcuts" },
      shortcutRow(g, "open", "Open", "Opens all apps"),
      shortcutRow(g, "hide", "Hide / show", "Hides the open apps, press again to show them"),
      shortcutRow(g, "quit", "Quit", "Quits the open apps")),
    el("div", { class: "rule", style: "margin-top:14px" }),
    el("div", { class: "apps-head" },
      el("span", {}, "Apps"),
      el("button", { class: "slash", onclick: () => addApps(g) }, "/ add apps")),
    g.apps.length
      ? el("div", {}, g.apps.map((a) => appRow(g, a)))
      : el("p", { class: "muted", style: "font-size:13px" }, `Add apps with “/ add apps”.`),
    el("div", { class: "bottom" },
      el("button", { class: "circle filled", title: "Open all", onclick: () => invoke("group_action", { id: g.id, action: "open" }) }, svg("arrow")),
      el("button", { class: "circle", title: "Hide all", onclick: () => invoke("group_action", { id: g.id, action: "hide" }) }, svg("eyeOff")),
      el("button", { class: "circle", title: "Quit all", onclick: () => invoke("group_action", { id: g.id, action: "quit" }) }, svg("x")),
      el("span", { class: "count muted" }, `${open} of ${g.apps.length} open`)),
  );
}

function updateErrors() {
  for (const err of detail.querySelectorAll("[data-err]")) {
    err.hidden = !failed.includes(`${selectedId}:${err.dataset.err}`);
  }
}

// MARK: Shortcut recorder

function shortcutRow(g, action, title, hint) {
  const btn = el("button", { class: "rec" });
  const paint = () => {
    const v = g.shortcuts[action];
    btn.className = "rec" + (v ? " set" : "");
    btn.textContent = v ? formatShortcut(v) : "Record shortcut";
  };
  paint();
  btn.onclick = () => record(btn, (accel) => { g.shortcuts[action] = accel; paint(); renderList(); scheduleSave(); }, paint);
  const clear = el("button", { class: "icon-btn", title: "Clear shortcut",
    onclick: () => { g.shortcuts[action] = null; paint(); renderList(); scheduleSave(); } }, svg("x"));
  const err = el("div", { class: "err", "data-err": action }, "Already used by another group or app. Try another.");
  err.hidden = !failed.includes(`${g.id}:${action}`);
  return el("div", {}, el("div", { class: "sc" }, el("span", {}, title), el("span", { class: "hint" }, hint), btn, clear), err);
}

const CODE_KEYS = { Space: "Space", Enter: "Enter", Tab: "Tab", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left",
  ArrowRight: "Right", Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Semicolon: ";", Quote: "'",
  Comma: ",", Period: ".", Slash: "/", Backslash: "\\", Backquote: "`", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown" };

function keyName(code) {
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^F\d{1,2}$/.test(code)) return code;
  return CODE_KEYS[code] ?? null;
}

async function record(btn, onSet, onCancel) {
  await invoke("suspend_shortcuts");
  btn.className = "rec recording";
  btn.textContent = "Press shortcut…";
  const finish = async () => {
    window.removeEventListener("keydown", onKey, true);
    await invoke("resume_shortcuts");
  };
  const onKey = (e) => {
    e.preventDefault();
    e.stopPropagation();
    if (e.code === "Escape") { finish(); onCancel(); return; }
    if ((e.code === "Backspace" || e.code === "Delete") && !e.metaKey && !e.ctrlKey && !e.altKey) { finish(); onSet(null); return; }
    const key = keyName(e.code);
    if (!key) return; // only modifiers so far
    const fKey = /^F\d/.test(key);
    if (!fKey && !(e.metaKey || e.ctrlKey || e.altKey)) return; // needs ⌘, ⌥ or ⌃ (Ctrl/Alt/Win)
    const parts = [];
    if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    if (e.metaKey) parts.push(isMac ? "Command" : "Super");
    finish();
    onSet([...parts, key].join("+"));
  };
  window.addEventListener("keydown", onKey, true);
}

// MARK: Apps

const BROWSER = /chrome|edge|brave|vivaldi|chromium|firefox|zen/i;

function appRow(g, a) {
  const tile = el("div", { class: "tile" }, (a.name || "?")[0].toUpperCase());
  iconFor(a.path).then((src) => { if (src) tile.replaceWith(el("img", { src, alt: "" })); });
  const state = running.get(a.path);
  const row = el("div", { class: "app" },
    el("div", { class: "app-line" }, tile,
      el("span", { class: "label" }, a.name,
        el("span", { class: "dot", style: state ? `background:var(--ink);opacity:${state.hidden ? .35 : 1}` : "" })),
      el("button", { class: "icon-btn", title: "Remove from group",
        onclick: () => { g.apps = g.apps.filter((x) => x.id !== a.id); renderDetail(); scheduleSave(); } }, svg("minus"))));
  if (BROWSER.test(a.path)) row.append(profilesEditor(a));
  return row;
}

/** Browser: one section per profile, each with its own URLs. */
function profilesEditor(a) {
  if (!a.profiles.length) a.profiles.push({ id: crypto.randomUUID(), profile: null, profileName: null, urls: [] });
  const wrap = el("div", { class: "profiles" });
  for (const p of a.profiles) {
    const profileInput = el("input", { value: p.profileName ?? p.profile ?? "", placeholder: "Last used", spellcheck: "false" });
    profileInput.oninput = () => { const v = profileInput.value.trim(); p.profile = v || null; p.profileName = v || null; scheduleSave(); };
    const section = el("div", { class: "profile" },
      el("div", { class: "profile-head" },
        el("span", {}, "Profile"),
        el("label", { class: "field profile-input" }, profileInput),
        a.profiles.length > 1 ? el("button", { class: "icon-btn", title: "Remove this profile and its URLs",
          onclick: () => { a.profiles = a.profiles.filter((x) => x.id !== p.id); renderDetail(); scheduleSave(); } }, svg("x")) : null));
    p.urls.forEach((u, i) => {
      const input = el("input", { value: u, placeholder: "mail.google.com", spellcheck: "false" });
      input.oninput = () => { p.urls[i] = input.value; scheduleSave(); };
      section.append(el("div", { class: "url" },
        el("label", { class: "field" }, el("span", { class: "muted" }, "/"), input),
        el("button", { class: "icon-btn", title: "Remove URL", onclick: () => { p.urls.splice(i, 1); renderDetail(); scheduleSave(); } }, svg("minus"))));
    });
    section.append(plusLabel("Add URL", () => { p.urls.push(""); renderDetail(); }));
    wrap.append(section);
  }
  wrap.append(el("div", { class: "rule soft", style: "margin:6px 0" }),
    plusLabel("Add another profile", () => { a.profiles.push({ id: crypto.randomUUID(), profile: null, profileName: null, urls: [] }); renderDetail(); }));
  return wrap;
}

function plusLabel(text, onclick) {
  return el("button", { class: "plus-label", onclick }, el("span", { class: "ring" }, svg("plus")), text);
}

async function addApps(g) {
  const picked = await window.__TAURI__.dialog.open({
    multiple: true,
    directory: false,
    defaultPath: platform.defaultAppDir ?? undefined,
    filters: [{ name: "Apps", extensions: platform.appExtensions }],
  });
  const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
  for (const path of paths) {
    if (g.apps.some((a) => a.path === path)) continue;
    g.apps.push(await invoke("new_app_entry", { path }));
  }
  renderDetail();
  scheduleSave();
}

// MARK: Groups

async function addGroup() {
  const used = new Set(groups.flatMap((g) => Object.values(g.shortcuts)).filter(Boolean));
  const n = [1, 2, 3, 4, 5, 6, 7, 8, 9].find((k) => !used.has(defaultShortcuts(k).open));
  const group = { id: crypto.randomUUID(), name: `Group ${groups.length + 1}`, apps: [],
                  shortcuts: n ? defaultShortcuts(n) : { open: null, hide: null, quit: null } };
  groups.push(group);
  selectedId = group.id;
  renderList();
  renderDetail();
  scheduleSave(true);
}

function removeGroup() {
  const i = groups.findIndex((g) => g.id === selectedId);
  if (i < 0) return;
  groups.splice(i, 1);
  selectedId = groups[Math.min(i, groups.length - 1)]?.id ?? null;
  renderList();
  renderDetail();
  scheduleSave();
}

listen("select-group", (e) => { selectedId = e.payload; renderList(); renderDetail(); });
listen("groups-changed", () => {
  // Reload when another window (the panel) changed the groups, not after our own save.
  if (!saveTimer && Date.now() - lastLocalSave > 1000) load();
});
load();
