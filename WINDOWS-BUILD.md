# Unofficial Windows build of Claudepot

This repository is a **redistribution of a Windows binary build** of
[`xiaolai/claudepot-app`](https://github.com/xiaolai/claudepot-app),
originally written by **Li Xiaolai** and licensed under the ISC License
(see [`LICENSE`](LICENSE), which is retained unchanged).

- **Not affiliated with, endorsed by, or supported by the upstream author.**
- The source tree here is a copy of upstream `main`; only this file and
  `INSTALL_LOG.md` are added.
- For the authoritative source, issues, and official releases, go to
  [`xiaolai/claudepot-app`](https://github.com/xiaolai/claudepot-app).

## What is in the Release

The compiled Windows binaries are attached to the GitHub **Release**, not
committed to the git tree:

| File | What it is |
|---|---|
| `claudepot-tauri.exe` | The desktop app (Tauri GUI). ~56 MB. Double-click to run. |
| `claudepot.exe` | The `claudepot` command-line tool. ~20 MB. |

## How it was built

Built on Windows 11 with the **GNU (MinGW-w64) Rust toolchain**, not the
MSVC toolchain the upstream README assumes, because this machine had no
Visual Studio C++ Build Tools. Full log in [`INSTALL_LOG.md`](INSTALL_LOG.md).

```
rustup default stable-x86_64-pc-windows-gnu
# MinGW-w64 (dlltool/gcc/ld) via: winget install BrechtSanders.WinLibs.POSIX.UCRT
cargo build -p claudepot-cli --release
cp target/release/claudepot.exe src-tauri/binaries/claudepot-cli-x86_64-pc-windows-gnu.exe
pnpm install
pnpm tauri build --no-bundle
# -> target/release/claudepot-tauri.exe
```

Known cosmetic warning during linking (does not affect the running app):
`.rsrc merge failure: multiple non-default manifests`.

## Caveats

- `--no-bundle` build: this is a bare `.exe`, with no installer, no Start
  Menu entry, and no code signing. Windows SmartScreen will warn on first run.
- Unofficial toolchain: if you hit a bug the upstream MSVC build does not
  have, that is on this build, not on upstream. Rebuild from
  [`xiaolai/claudepot-app`](https://github.com/xiaolai/claudepot-app) to compare.
- Data lives in `%USERPROFILE%\.claudepot\` (override with `CLAUDEPOT_DATA_DIR`).
- Proxy: the app reads `HTTPS_PROXY` / `https_proxy` / `ALL_PROXY` from the
  environment for its outbound HTTPS calls (not `HTTP_PROXY`). Set it as a
  persistent user environment variable if you launch by double-click.
