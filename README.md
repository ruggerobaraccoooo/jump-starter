# Jump Starter (Tauri)

Cross-platform version of Jump Starter (macOS · Windows · Linux). The native macOS app lives in `~/app-groups`.

## Run on this Mac

```bash
npm run tauri dev
```

Opens the tray/menu bar icon; click it for the panel. Quit the native Jump Starter first, otherwise both apps
try to register the same shortcuts (the editor then shows "Already used…").

On first launch on a Mac, groups are imported from the native app.
Data: `~/Library/Application Support/local.jumpstarter.tauri/groups.json`.

## Structure

- `src/` — UI (plain HTML/CSS/JS): `panel.html/js` (tray panel), `editor.html/js`, `styles.css`, `common.js`
- `src-tauri/src/lib.rs` — tray, windows, global shortcuts, commands
- `src-tauri/src/model.rs` — groups / apps / shortcuts storage
- `src-tauri/src/platform/` — per-OS layer:
  - `macos.rs` — NSWorkspace / NSRunningApplication (same as the Swift app)
  - `windows.rs` — .exe or Start menu .lnk (resolved via IShellLink); bring to front / ShellExecute;
    hide = minimize windows, show = restore; quit = WM_CLOSE to each window; icons via PrivateExtractIcons
  - `linux.rs` — .desktop entries (Name/Exec/Icon) or programs/AppImages, Flatpak aware; open via `gio launch`;
    hide/show via X11 (`WM_CHANGE_STATE` / `_NET_ACTIVE_WINDOW`), no-op on pure Wayland; quit = SIGTERM / `flatpak kill`

## Debug helpers (env vars)

`JUMP_OPEN_PANEL=1`, `JUMP_KEEP_OPEN=1`, `JUMP_OPEN_EDITOR=1`, `JUMP_DEMO_EXPAND=1` (slow expand demo).

## Checking other platforms from the Mac

```bash
cd src-tauri && JUMP_CHECK_ONLY=1 cargo check --target x86_64-pc-windows-msvc
```

`JUMP_CHECK_ONLY` skips Tauri's resource step (needs `llvm-rc` when cross-compiling). Linux can't be checked
through the full app on a Mac (needs GTK/WebKit libraries); the Linux layer compiles on its own.

## Next steps

5. GitHub Actions builds for Windows/Linux installers; test in UTM VMs (Windows 11 ARM, Ubuntu ARM).
- Not yet: Microsoft Store (UWP) apps on Windows, "add running app", browser profile picker, Launch at login.
