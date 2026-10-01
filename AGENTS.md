# AGENTS.md

Tauri 2 desktop app (Windows/macOS) managing `ssh -L` local forwards.
Detailed, verified docs live in `README.md` — trust code + README over memory.

## Layout & entrypoints

- `src/` — React 19 + TS (strict) + Tailwind 4, Vite (no bundler magic, `devUrl: localhost:5173`).
  Entry `src/main.tsx` → `src/App.tsx`; `src/lib/api.ts` = typed FFI wrappers; `src/components/ui/` = shadcn-style primitives.
- `src-tauri/src/` — `main.rs` (entry → `lib.rs::run()`), `ssh_config.rs` (config parser),
  `tunnel.rs` (lifecycle core), `store.rs` (JSON persistence), `process.rs` (cross-platform process),
  `commands.rs` (FFI layer), `tray.rs`, `logging.rs`, `error.rs` (user-facing errors).
- `scripts/` — `gen-icons.mjs` (icon codegen), `uiclick.ps1` / `screenshot.ps1` (UIA verification helpers).

## Commands (Windows PowerShell shells do NOT auto-load the user profile)

- Node is managed by **fnm**; run this prefix before any `pnpm` command:
  `fnm env --use-on-cd --shell powershell | Out-String | Invoke-Expression`
- Package manager is **pnpm** — never npm/yarn. Allowed build scripts live in
  `pnpm-workspace.yaml` → `allowBuilds:` (pnpm 11+ removed `package.json#pnpm.onlyBuiltDependencies`;
  `ERR_PNPM_IGNORED_BUILDS` means this key is wrong/missing).
- **Cargo needs the proxy** or crates.io crawls: `$env:CARGO_HTTP_PROXY="http://127.0.0.1:7897"`.
  Same value for `HTTPS_PROXY`/`HTTP_PROXY` when using git/gh.
- Verify (order doesn't matter, both are required):
  - `cargo check --manifest-path src-tauri/Cargo.toml`
  - `cargo test --manifest-path src-tauri/Cargo.toml` (9 tests; one spawns PowerShell ≈1.4 s;
    filter: `cargo test ... <name>`)
  - `pnpm build` (= `tsc -b` strict + vite production build)
- Dev: `pnpm tauri dev`. Release installers: `pnpm tauri build` →
  `src-tauri/target/release/bundle/{msi,nsis}/` (2–6 min; editing `tauri.conf.json` forces a
  tauri-codegen rebuild).
- Icons: `node scripts/gen-icons.mjs` writes `src-tauri/icons/` — tauri-build hard-fails if
  `src-tauri/icons/icon.ico` is missing (must be under `src-tauri/`, not repo root).

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
- Frameless window (`decorations:false`): titlebar buttons need the permissions in
  `src-tauri/capabilities/default.json` (minimize/toggle-maximize/hide/is-maximized/start-dragging).
  Buttons silently doing nothing = missing permission.
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
