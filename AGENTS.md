# AGENTS.md

Tauri 2 desktop app (Windows/macOS) managing `ssh -L` local forwards.
Detailed, verified docs live in `README.md` — trust code + README over memory.

## Layout & entrypoints

- `src/` — React 19 + TS (strict) + Tailwind 4, Vite (no bundler magic, `devUrl: localhost:5173`).
  Entry `src/main.tsx` → `src/App.tsx`; `src/lib/api.ts` = typed FFI wrappers; `src/components/ui/` = shadcn-style primitives.
- `src-tauri/src/` — `main.rs` (entry → `lib.rs::run()`), `ssh_config.rs` (config parser),
  `tunnel.rs` (lifecycle core), `store.rs` (JSON persistence), `process.rs` (cross-platform process),
  `commands.rs` (FFI layer), `tray.rs`, `logging.rs`, `error.rs` (user-facing errors).
- `scripts/` — `gen-icons.mjs` (icon codegen), `verify-icons.mjs` (icon colour/geometry audit),
  `uiclick.ps1` / `screenshot.ps1` (UIA verification helpers).

## Commands (Windows PowerShell shells do NOT auto-load the user profile)

- Node is managed by **fnm**; run this prefix before any `pnpm` command:
  `fnm env --use-on-cd --shell powershell | Out-String | Invoke-Expression`
- Package manager is **pnpm** for all local work — never npm/yarn. Allowed build scripts live in
  `pnpm-workspace.yaml` → `allowBuilds:` (pnpm 11+ removed `package.json#pnpm.onlyBuiltDependencies`;
  `ERR_PNPM_IGNORED_BUILDS` means this key is wrong/missing).
  The one npm exception is regenerating `package-lock.json` for CI (see Conventions).
- **Cargo needs the proxy** or crates.io crawls: `$env:CARGO_HTTP_PROXY="http://127.0.0.1:7897"`.
  Same value for `HTTPS_PROXY`/`HTTP_PROXY` when using git/gh.
- Verify (order doesn't matter, both are required):
  - `cargo check --manifest-path src-tauri/Cargo.toml`
  - `cargo test --manifest-path src-tauri/Cargo.toml` (9 tests; one spawns PowerShell ≈1.4 s;
    filter: `cargo test ... <name>`)
  - `pnpm build` (= `tsc -b` strict + vite production build)
- Dev: `pnpm tauri dev`. Release installers: `pnpm tauri build` →
  `src-tauri/target/release/bundle/{msi,nsis}/` (2–6 min; editing `tauri.conf.json` **or
  `tauri.windows.conf.json`** forces a tauri-codegen rebuild).
- Icons: `node scripts/gen-icons.mjs` writes `src-tauri/icons/` — tauri-build hard-fails if
  `src-tauri/icons/icon.ico` is missing (must be under `src-tauri/`, not repo root).
  The `.icns` is rendered on **Apple's 824-on-1024 grid** (824×824 squircle, r=185.4, centered,
  100px transparent margin) while PNG/ICO stay full-bleed: macOS 26 (Tahoe) shrinks non-conforming
  icons into a gray rounded frame ("icon jail"), and full-bleed art also reads oversized in the Dock.
  Verify with `node scripts/verify-icons.mjs` (colour stats + opaque-region margin; `.icns` entries
  must report 9.77%, PNG/ICO 0%). True Liquid Glass needs an `.icon` → `Assets.car` build
  (Xcode 26 `actool`, macOS-only); Tauri ≥2.11 accepts a `.icon`/`Assets.car` in `bundle.icon`
  (tauri-apps/tauri#14671).
  `icons/tray-icon.png` is the **macOS menu-bar template image** (36×36, black + alpha, arrow
  knocked out): the menu bar must never get the coloured app icon, so `tray.rs` embeds it with
  `include_bytes!` and sets `icon_as_template(true)` on macOS while Windows keeps the coloured icon.
  Because the tray icon lives in `icons/`, regenerating icons is the only way to change it.

## Gotchas that will bite you

- **Never write source files with `Add-Content`/`Set-Content`** — PS 5.1 defaults to ANSI/GBK and
  corrupts UTF-8 (has corrupted `tunnel.rs`). Use the edit/write tools, or
  `[IO.File]::WriteAllText($p, $text, [Text.UTF8Encoding]::new($false))`.
- Console output garbles Chinese and the image-read tool can serve stale bytes for a repeated
  path — verify by content (hash/pixel/byte compare), not by display.
- **Tunnel lifecycle**: `stop_tunnel` KEEPS the record (`pid=0` → 已停止); only `remove_tunnel`
  deletes, and only stopped records; `update_tunnel` edits stopped records only; `restart_tunnel`
  reuses saved config. Do not "fix" stop into deleting.
- **Success criterion = local `bind:port` is LISTENING** (polled every 150 ms; ssh binds `-L`
  only after auth). Not a time window. 18 s deadline, ssh `ConnectTimeout=15`, password precheck
  (`ssh -T`) 5 s fail-open. "Success toast then immediate tunnel-exited" is a regression.
- **Password flow**: backend signals with error prefix `PASSWORD_REQUIRED::` → frontend
  `PasswordDialog` (helpers `isPasswordRequired` in `src/lib/api.ts`). The password travels only
  as child env (`SSH_ASKPASS` + `STM_ASKPASS_PWD`). `main.rs` has an askpass helper mode: when
  called with an argument and `SSH_ASKPASS` set, it must print ONLY the password + `\n` to
  stdout and exit — any other stdout (logging) corrupts the password.
- FFI is snake_case both directions (serde defaults, no `rename_all`): JS uses `local_port`;
  invoke args are `{ request, password, id }`.
- `AppState.store` is a `std::sync::Mutex` — never hold the guard across an `.await`.
- **Windowing is per-platform**: base `tauri.conf.json` keeps `decorations: true` so macOS/Linux
  get the native titlebar (traffic lights); only `tauri.windows.conf.json` sets
  `decorations: false`, and `TitleBar` renders only when `usesNativeTitleBar()` (`src/lib/platform.ts`)
  is false — never render it on macOS (two titlebars). Platform configs merge with JSON Merge Patch
  (RFC 7396), so **arrays are replaced wholesale**: the `app.windows` object is duplicated in both
  files and window fields must be edited in both.
- Frameless window (Windows): titlebar buttons need the permissions in
  `src-tauri/capabilities/default.json` (minimize/toggle-minimize/toggle-maximize/hide/is-maximized/start-dragging).
  Buttons silently doing nothing = missing permission.
- **macOS menu-bar (tray) icons must be monochrome template images** (black + alpha, with the glyph
  knocked out of the fill, rendered via `icon_as_template(true)`): Apple's menu bar expects black/white
  glyphs that the system inverts for light/dark, so the coloured app icon must not go there. If the
  tray icon looks like a featureless solid block, the fill was not knocked out (template rendering only
  reads alpha).
- Windows process logic shells out (`tasklist`/`taskkill`/PowerShell); parse structurally
  (pid columns), never localized text.
- App data is outside the repo: `%APPDATA%\ssh-tunnel-manager\{tunnels.json,app.log}`.
  Manual tests pollute it; the JSON is safe to empty between sessions.
- Dev server owns Vite's default port 5173: a tunnel using local port 5173 under
  `pnpm tauri dev` will (correctly) fail with a port-in-use error that names the node process.

## UI verification helpers

- WebView2 exposes the DOM through UIA: scope searches to the `Document` element, filter by
  `ControlType.Button` (table cells share the same accessible name as buttons, e.g. 关闭),
  and re-enumerate after actions — element snapshots go stale.
- `scripts/uiclick.ps1 -WindowTitle "..." -ElementName "..."`, `scripts/screenshot.ps1 -Path ...`.

## Conventions

- UI copy and all user-facing errors are Chinese (`AppError` variants are shown verbatim).
- Conventional commits (`feat:`/`fix:`/`chore:`/`docs:`) with English summaries;
  push verified changes to `origin/main` (github.com/hmilyld/SSHTunnel, public).
- Don't commit screenshots/build artifacts (`node_modules`, `dist`, `src-tauri/target`,
  `src-tauri/gen/schemas` are gitignored).
- **Releasing = push a `vX.Y.Z` tag.** `.github/workflows/release.yml` then builds the
  installers (Windows NSIS `.exe` + WiX `.msi`, macOS `.dmg`) and publishes a GitHub Release.
  Bump `version` in `package.json` **and** `src-tauri/tauri.conf.json` (Cargo.toml too, for
  consistency) first: the workflow hard-fails when the tag and those files disagree.
- **Two package managers on purpose, and two lockfiles to keep in sync:**
  - CI (GitHub Actions) uses **npm** — `npm ci`, driven by the committed `package-lock.json`.
  - Local development uses **pnpm** — `pnpm install`, driven by `pnpm-lock.yaml`.
  - After changing dependencies run `pnpm add/remove …` **and** regenerate the npm lockfile
    (`npm install --package-lock-only` in a clean dir holding just `package.json`, then copy the
    file back — running npm directly in this workspace can fail on pnpm's `node_modules` layout).
    If only one is updated, CI fails fast at `npm ci` instead of silently installing different
    versions. `tauri.conf.json` → `beforeBuildCommand` is `npm run build` on purpose: it must also
    work in CI, where only npm exists. Local installs stay pnpm-driven; `npm run build` reads the
    same `package.json` scripts, and `beforeDevCommand` remains `pnpm dev`.
- Copyright/author metadata lives in three places and must stay in sync:
  `package.json` (`author`/`homepage`), `src-tauri/Cargo.toml` (`authors`/`homepage`), and the
  footer line in `src/components/StatusBar.tsx` (`© 2026 hmilyld.com`).
- The footer's data-folder button calls `reveal_data_dir`, which opens the directory the
  **backend** resolves (`Store::data_dir`); never add a command that opens a caller-supplied path.
