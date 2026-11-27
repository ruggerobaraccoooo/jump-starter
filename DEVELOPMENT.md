# Developing Jump Starter

Cross-platform version built with [Tauri 2](https://tauri.app) (Rust core, plain HTML/CSS/JS interface).

## Run on a Mac

```bash
npm install
npm run tauri dev
```

Opens the tray / menu bar icon; click it for the panel. If the native macOS Jump Starter is running too, quit it
first: both would register the same shortcuts (the editor then shows "Already used…").

On first launch on a Mac, groups are imported from the native app. Data:
`~/Library/Application Support/local.jumpstarter.tauri/groups.json`.

## Structure

- `src/` — interface: `panel.html/js` (tray panel), `editor.html/js`, `styles.css`, `common.js`
- `src-tauri/src/lib.rs` — tray, windows, global shortcuts, commands
- `src-tauri/src/model.rs` — groups / apps / shortcuts storage
- `src-tauri/src/platform/` — per-OS layer:
  - `macos.rs` — NSWorkspace / NSRunningApplication
  - `windows.rs` — .exe or Start menu .lnk (resolved via IShellLink); bring to front / ShellExecute;
    hide = minimize windows, show = restore; quit = WM_CLOSE to each window; icons via PrivateExtractIcons
  - `linux.rs` — .desktop entries (Name/Exec/Icon) or programs/AppImages, Flatpak aware; open via `gio launch`;
    hide/show via X11 (`WM_CHANGE_STATE` / `_NET_ACTIVE_WINDOW`), no-op on pure Wayland; quit = SIGTERM / `flatpak kill`

## Checking Windows code from a Mac

```bash
cd src-tauri && JUMP_CHECK_ONLY=1 cargo check --target x86_64-pc-windows-msvc
```

`JUMP_CHECK_ONLY` skips Tauri's resource step (needs `llvm-rc` when cross-compiling).

## Builds and releases

- Every push to `main` builds installers for Windows, Linux (x64 + ARM) and macOS
  (`.github/workflows/build.yml`, downloads under Actions → Artifacts).
- `./scripts/release.sh 0.2.0` sets the version, tags `v0.2.0` and pushes; `.github/workflows/release.yml`
  then builds everything and publishes a GitHub Release with the installers and `.github/release-notes.md`.
- Testing in virtual machines: see [TESTING.md](TESTING.md).

## Debug helpers (environment variables)

| Variable | Effect |
|---|---|
| `JUMP_OPEN_PANEL=1` | open the panel at launch |
| `JUMP_KEEP_OPEN=1` | don't hide the panel when it loses focus |
| `JUMP_OPEN_EDITOR=1` | open the editor at launch |
| `JUMP_EXPAND_GROUP=<n>` | expand group n (0-based) after launch |
| `JUMP_THEME=dark\|light` | force the theme |
| `JUMP_DATA_DIR=<folder>` | read groups from another folder (demo data for screenshots) |
| `JUMP_DEMO_EXPAND=1` | slow-motion expand/collapse demo |
