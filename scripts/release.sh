#!/bin/zsh
# Publish a new version: ./scripts/release.sh 0.2.0
# Sets the version in package.json, tauri.conf.json and Cargo.toml, commits, tags v<version>
# and pushes. GitHub Actions then builds all installers and publishes the Release.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:?Usage: scripts/release.sh <version>, e.g. 0.2.0}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Version must look like 1.2.3" >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo "Commit or stash your changes first." >&2; exit 1; }
git rev-parse "v$VERSION" >/dev/null 2>&1 && { echo "Tag v$VERSION already exists." >&2; exit 1; }

node -e '
  const fs = require("fs"), v = process.argv[1];
  for (const f of ["package.json", "src-tauri/tauri.conf.json"]) {
    const j = JSON.parse(fs.readFileSync(f, "utf8")); j.version = v;
    fs.writeFileSync(f, JSON.stringify(j, null, 2) + "\n");
  }
  const toml = fs.readFileSync("src-tauri/Cargo.toml", "utf8")
    .replace(/^version = ".*"$/m, `version = "${v}"`);
  fs.writeFileSync("src-tauri/Cargo.toml", toml);
' "$VERSION"
(cd src-tauri && cargo update -p jump-starter --offline >/dev/null 2>&1 || true)

if [[ -n "$(git status --porcelain)" ]]; then
  git commit -qam "Release v$VERSION"
fi
git tag -a "v$VERSION" -m "Jump Starter v$VERSION"
git push -q origin HEAD
git push -q origin "v$VERSION"
echo "Pushed v$VERSION. Follow the build: https://github.com/$(git remote get-url origin | sed -E 's#.*github.com[:/](.*)\.git#\1#')/actions"
