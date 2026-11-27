# Testing Jump Starter on Windows and Linux (from an Apple silicon Mac)

## 1. Get the installers

Every push to `main` builds installers on GitHub (Actions → **Build**). Open the latest run and download from
**Artifacts**:

| Artifact | Contains | Use it on |
|---|---|---|
| `jump-starter-windows` | `.msi` and `-setup.exe` | Windows 11 (also Windows 11 ARM, through its x64 emulation) |
| `jump-starter-linux-arm` | `.deb`, `.rpm`, `.AppImage` (ARM) | Ubuntu/Fedora ARM VM in UTM |
| `jump-starter-linux` | same for Intel/AMD | real PCs |
| `jump-starter-macos` | `.dmg` | Macs |

You can also start a build by hand: Actions → Build → **Run workflow**.

> The installers aren't code-signed yet, so Windows shows "Windows protected your PC" → **More info → Run anyway**.

## 2. Virtual machines with UTM (free)

Install UTM from <https://mac.getutm.app> (or `brew install --cask utm`).

### Windows 11 (ARM)

1. Download the **Windows 11 ARM64** ISO from Microsoft: <https://www.microsoft.com/software-download/windows11arm64>.
2. UTM → **Create a New Virtual Machine → Virtualize → Windows**, pick the ISO, tick **Install drivers and SPICE tools**.
   4 CPU cores, 8 GB RAM, 64 GB disk are plenty.
3. Install Windows. During setup you can choose "I don't have internet" to skip the Microsoft account (Shift+F10 →
   `oobe\bypassnro` if the option is hidden).
4. Drag the `.msi` into the VM (or download it there from GitHub) and install it.

### Ubuntu 24.04 (ARM)

1. Download **Ubuntu 24.04 Desktop for ARM64** from <https://ubuntu.com/download/desktop> (ARM link).
2. UTM → **Virtualize → Linux**, pick the ISO, 4 cores / 6 GB / 40 GB.
3. After installing: `sudo apt install gnome-shell-extension-appindicator` and log out/in, so the tray icon shows
   (GNOME hides tray icons without it).
4. Install the ARM `.deb`: `sudo apt install ./Jump.Starter_*_arm64.deb`.
5. Test both sessions from the login screen's gear menu: **Ubuntu on Xorg** (X11, hide/show works) and
   **Ubuntu** (Wayland, hide/show isn't possible there).

## 3. Checklist

- [ ] Tray icon appears; clicking it opens the panel (Windows: above the taskbar; Linux: below the top bar)
- [ ] Add a group in the editor, add apps (Windows: Start menu shortcuts; Linux: `/usr/share/applications`)
- [ ] App icons show; running apps get a dot
- [ ] Open shortcut (default Ctrl+1 on Windows, Ctrl+Alt+1 on Linux) opens the group; already-running apps come to the front
- [ ] Hide / show shortcut (Ctrl+Alt+1 on Windows, Ctrl+Alt+Shift+1 on Linux) minimizes, then restores the group's windows
- [ ] Quit button closes the apps (Windows apps may ask to save)
- [ ] Chrome/Edge/Firefox with profile + URLs opens the right profile
- [ ] Expand animation is smooth; Esc or clicking outside closes the panel
- [ ] Dark mode follows the system setting
