import { invoke, listen, formatShortcut, appIcon, el, svg, defaultShortcuts } from "./common.js";

let groups = [];
let running = new Map();
let expandedId = null;
const list = document.getElementById("groups");

document.getElementById("add").append(svg("plus"));
document.getElementById("edit").append(svg("pencil"), "Edit groups");
document.getElementById("edit").onclick = () => invoke("open_editor", { groupId: expandedId ?? groups[0]?.id ?? null });
document.getElementById("quit").onclick = () => invoke("quit_jump_starter");
document.getElementById("add").onclick = addGroup;

// Clicks in the transparent area around the panel, or Esc, close it like a menu.
document.body.addEventListener("mousedown", (e) => { if (!e.target.closest(".panel")) invoke("hide_panel"); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") invoke("hide_panel"); });
document.addEventListener("contextmenu", (e) => e.preventDefault());

async function load() {
  groups = await invoke("get_groups");
  await refreshRunning(false);
  render();
}

async function refreshRunning(rerender = true) {
  const list = await invoke("running_apps");
  const next = new Map(list.map((r) => [r.path, r]));
  const changed = JSON.stringify([...next]) !== JSON.stringify([...running]);
  running = next;
  if (changed && rerender) render();
}

function render() {
  list.replaceChildren();
  if (!groups.length) {
    list.append(el("div", { class: "empty" },
      el("h2", {}, "Start your first group"),
      el("p", { class: "muted" }, "Collect apps you use together and open them with one shortcut."),
      el("button", { class: "slash", onclick: addGroup }, "/ create a group")));
    return;
  }
  groups.forEach((g, i) => {
    list.append(groupRow(g));
    if (i < groups.length - 1) {
      const gone = g.id === expandedId || groups[i + 1].id === expandedId;
      list.append(el("div", { class: "divider" + (gone ? " gone" : ""), "data-between": `${g.id}|${groups[i + 1].id}` }));
    }
  });
}

function groupRow(g) {
  const open = g.apps.filter((a) => running.has(a.path)).length;
  const pct = g.apps.length ? (open / g.apps.length) * 100 : 0;
  const row = el("div", { class: "group" + (g.id === expandedId ? " expanded on-accent" : ""), "data-id": g.id });

  const head = el("div", { class: "g-head", onclick: () => toggle(g.id) },
    el("span", { class: "g-name" }, g.name),
    g.shortcuts.open ? el("span", { class: "pill" + (g.id === expandedId ? " active" : "") }, formatShortcut(g.shortcuts.open)) : null);
  const chev = svg("chevron"); chev.classList.add("chev"); head.append(chev);

  const status = el("div", { class: "reveal" }, el("div", {},
    el("div", { class: "status" }, `${open} of ${g.apps.length} open`),
    el("div", { class: "progress" }, el("div", { style: `width:${pct}%` }))));

  const apps = g.apps.length
    ? el("div", { class: "g-apps" },
        el("div", { class: "icons" }, g.apps.map((a) => appIcon(a, running, () => invoke("app_action", { path: a.path, action: "open" })))),
        circle("arrow", true, "Open all", () => invoke("group_action", { id: g.id, action: "open" })))
    : el("div", { class: "g-apps" }, el("button", { class: "slash", onclick: (e) => { e.stopPropagation(); invoke("open_editor", { groupId: g.id }); } }, "/ add apps"));

  const actions = el("div", { class: "reveal" }, el("div", {}, el("div", { class: "actions" },
    el("button", { class: "slash", onclick: (e) => { e.stopPropagation(); invoke("open_editor", { groupId: g.id }); } }, "/ edit"),
    circle("eyeOff", false, "Hide all" + (g.shortcuts.hide ? ` (${formatShortcut(g.shortcuts.hide)} hides / shows)` : ""),
           () => invoke("group_action", { id: g.id, action: "hide" })),
    circle("x", false, "Quit all" + (g.shortcuts.quit ? ` (${formatShortcut(g.shortcuts.quit)})` : ""),
           () => invoke("group_action", { id: g.id, action: "quit" })))));

  row.append(head, status, apps, actions);
  row.addEventListener("click", () => { if (g.id !== expandedId) toggle(g.id); });
  return row;
}

function circle(icon, filled, title, onclick) {
  return el("button", { class: "circle" + (filled ? " filled" : ""), title, onclick: (e) => { e.stopPropagation(); onclick(); } }, svg(icon));
}

/** Morph in place: only classes change, so CSS transitions animate everything. */
function toggle(id) {
  expandedId = expandedId === id ? null : id;
  for (const row of list.querySelectorAll(".group")) {
    const on = row.dataset.id === expandedId;
    row.classList.toggle("expanded", on);
    row.classList.toggle("on-accent", on);
    row.querySelector(".pill")?.classList.toggle("active", on);
  }
  for (const d of list.querySelectorAll(".divider")) {
    const [a, b] = d.dataset.between.split("|");
    d.classList.toggle("gone", a === expandedId || b === expandedId);
  }
}

async function addGroup() {
  const used = new Set(groups.flatMap((g) => Object.values(g.shortcuts)).filter(Boolean));
  const n = [1, 2, 3, 4, 5, 6, 7, 8, 9].find((k) => !used.has(defaultShortcuts(k).open));
  const shortcuts = n ? defaultShortcuts(n) : { open: null, hide: null, quit: null };
  const group = { id: crypto.randomUUID(), name: `Group ${groups.length + 1}`, apps: [], shortcuts };
  groups.push(group);
  await invoke("save_groups", { groups });
  invoke("open_editor", { groupId: group.id });
}

listen("groups-changed", load);
listen("panel-shown", (e) => {
  document.body.classList.toggle("anchor-bottom", e.payload === "bottom");
  refreshRunning();
});
setInterval(() => { if (document.visibilityState === "visible") refreshRunning(); }, 1500);
load();
