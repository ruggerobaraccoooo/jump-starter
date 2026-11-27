// Shared helpers for the panel and the editor.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

export { invoke, listen };

export const platform = await invoke("platform_info");
export const isMac = platform.os === "macos";

/** "Control+Alt+Shift+Command+1" → "⌃⌥⇧⌘1" on macOS, "Ctrl+Alt+Shift+1" elsewhere. */
export function formatShortcut(accel) {
  if (!accel) return "";
  const parts = accel.split("+");
  const key = parts.pop();
  const has = (m) => parts.some((p) => p.toLowerCase() === m.toLowerCase());
  if (isMac) {
    return (has("Control") || has("Ctrl") ? "⌃" : "") + (has("Alt") || has("Option") ? "⌥" : "") +
           (has("Shift") ? "⇧" : "") + (has("Command") || has("Cmd") || has("Super") ? "⌘" : "") + key;
  }
  const names = [];
  if (has("Control") || has("Ctrl")) names.push("Ctrl");
  if (has("Alt")) names.push("Alt");
  if (has("Shift")) names.push("Shift");
  if (has("Super") || has("Command")) names.push("Win");
  return [...names, key].join("+");
}

/** Default shortcuts for group number n (1…9): Control-based on macOS and Windows. */
export function defaultShortcuts(n) {
  if (isMac || platform.os === "windows") {
    return { open: `Control+${n}`, hide: `Control+Alt+${n}`, quit: `Control+Shift+${n}` };
  }
  // Linux desktops use Ctrl+number for workspaces and tabs, so add Alt there.
  return { open: `Control+Alt+${n}`, hide: `Control+Alt+Shift+${n}`, quit: null };
}

const iconCache = new Map();
export async function iconFor(path) {
  if (!iconCache.has(path)) iconCache.set(path, invoke("app_icon", { path }).catch(() => null));
  return iconCache.get(path);
}

export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else if (v !== undefined && v !== null && v !== false) node.setAttribute(k, v);
  }
  for (const c of children.flat()) if (c !== null && c !== undefined && c !== false) node.append(c);
  return node;
}

const ICONS = {
  arrow: '<path d="M7 17 17 7M9 7h8v8"/>',
  eyeOff: '<path d="M3 3l18 18M10.6 10.6a2 2 0 0 0 2.8 2.8M9.9 5.1A10 10 0 0 1 12 5c5 0 9 4.5 10 7a13 13 0 0 1-3.2 4.2M6.6 6.6A13 13 0 0 0 2 12c1 2.5 5 7 10 7a9.7 9.7 0 0 0 5.4-1.6"/>',
  x: '<path d="M6 6l12 12M18 6 6 18"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  chevron: '<path d="m6 9 6 6 6-6"/>',
  pencil: '<path d="M4 20h4L19 9l-4-4L4 16v4z"/>',
  gear: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1"/>',
};
export function svg(name) {
  const s = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  s.setAttribute("viewBox", "0 0 24 24");
  s.setAttribute("fill", "none");
  s.setAttribute("stroke", "currentColor");
  s.setAttribute("stroke-width", "2");
  s.setAttribute("stroke-linecap", "round");
  s.setAttribute("stroke-linejoin", "round");
  s.innerHTML = ICONS[name];
  return s;
}

/** App icon (or a letter tile) with a running dot and an optional profile badge. */
export function appIcon(app, runningMap, onClick) {
  const state = runningMap.get(app.path);
  const node = el("div", { class: "app-icon" + (state ? (state.hidden ? " running hidden" : " running") : ""),
                           title: app.name + (state ? (state.hidden ? " (hidden)" : " (open)") : "") });
  const tile = el("div", { class: "tile" }, (app.name || "?").slice(0, 1).toUpperCase());
  node.append(tile);
  iconFor(app.path).then((src) => {
    if (src) tile.replaceWith(el("img", { src, alt: "" }));
  });
  const names = (app.profiles || []).map((p) => p.profileName || p.profile).filter(Boolean);
  if (names.length) node.append(el("span", { class: "badge" }, names.length > 1 ? String(names.length) : names[0][0].toUpperCase()));
  node.append(el("span", { class: "dot" }));
  if (onClick) node.addEventListener("click", (e) => { e.stopPropagation(); onClick(); });
  return node;
}
