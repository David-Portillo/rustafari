# CLAUDE.md

Context for AI agents working in this repository. Read this before making changes.

---

## What rustafari is

A **devtoy**: a single native desktop app bundling the small utilities a developer
would otherwise paste into a random website — JSON formatting, Base64, hashing,
UUIDs, URL encoding. Conceptually a Rust DevToys.

Two properties drive nearly every decision:

1. **Everything runs locally and offline.** Nothing pasted into the app leaves the
   machine. There is no network code anywhere in the tool logic, and there should
   never be. If a feature request implies a network call, flag it rather than
   quietly adding one.
2. **It ships as a standalone GUI app** for macOS, Windows and Linux, installable
   via `brew install --cask rustafari`. It is *not* a CLI, even though
   `cargo install rustafari` also works.

Owner: David Portillo (GitHub `David-Portillo`). Repo:
<https://github.com/David-Portillo/rustafari> (public).

---

## Hard constraints

These came from the owner directly. Do not relitigate them without asking.

| Constraint | Why |
| --- | --- |
| **No Chromium, no webview, no JavaScript** | Explicitly requested: "as lean as possible, no chromium". This is why the app uses `egui`/`eframe` and **not** Tauri, Dioxus-desktop, or anything else riding a system webview (WebView2 on Windows is Chromium). |
| **Lean binaries** | The universal macOS binary is ~6 MB, the DMG ~4 MB. Weigh any dependency against that. The icon font was subset from 854 KB to 8 KB rather than vendored whole. |
| **`rustafari-core` has zero UI dependencies** | It must stay usable from a CLI, a test harness, or anything else. Never `use eframe` or `egui` in it. |

Rejected alternatives, for the record: Tauri (webview/Chromium), Dioxus desktop
(webview), Slint (custom DSL, licensing friction).

---

## Layout

```
Cargo.toml                       workspace root; version + shared metadata live here
crates/
  rustafari-core/                pure tool logic, no UI dependency, fully unit tested
    src/spec.rs                  the Tool trait and the option model — read this first
    src/lib.rs                   all_tools(), matches_query(), contract tests
    src/tools/{json,base64,url,hash,uuid}.rs
  rustafari-app/                 egui/eframe desktop shell
    src/main.rs                  window setup, module declarations
    src/app.rs                   all UI
    src/settings.rs              user settings and their on-disk format
    src/theme.rs                 palette and egui Visuals
    src/icons.rs                 icon codepoints + font installation
    assets/lucide.ttf            subset icon font (8 KB)
    assets/LUCIDE-LICENSE        ISC, required attribution
packaging/
  macos/Info.plist               bundle metadata; __VERSION__ is substituted at build
  homebrew/rustafari.rb          canonical cask; CI copies it to the tap
scripts/
  bundle-macos.sh                universal .app + DMG, opt-in signing/notarization
  subset-icons.sh                regenerates the icon font from icons.rs
.github/workflows/
  ci.yml                         fmt + clippy + test on macOS/Linux/Windows
  release.yml                    on tag: installers, GitHub release, tap update, crates.io
```

---

## The central design decision

**Tools declare their options; they never draw them.** A tool provides
`ToolMeta` (identity + search terms), an `InputMode`, and a list of `OptionSpec`
(`Toggle` / `Choice` / `Number`). The frontend renders any tool generically from
that description.

The payoff: **adding a tool requires no UI code at all.**

### Adding a tool

1. Create `crates/rustafari-core/src/tools/<name>.rs` with a unit struct
   implementing `Tool`.
2. Register it in `src/tools/mod.rs` and in `all_tools()` in `src/lib.rs`.

That is the whole procedure. Three workspace-level tests in `lib.rs` then police
the new tool automatically:

- ids are unique,
- every `Choice` default names a real choice,
- running with default options and empty input is never an error (that is the
  state the user sees when they open a tool).

An icon is optional: `tool_icon()` in `app.rs` maps known ids to glyphs and falls
back per category, so an unmapped tool still looks right.

`Options` is built from the specs, so every getter has a default and **cannot
fail** — tool implementations never handle missing options.

### Conventions inside tools

- Errors are user-facing copy, not debug dumps. `ToolError::new("Not valid
  Base64: …")`, and include position information when the parser offers it.
- Empty input returns `Ok("")` for transforms. Hashes are the exception — the
  digest of the empty string is a real answer.
- Be forgiving about input shape where it costs nothing: Base64 decoding strips
  the whitespace that line-wrapped input arrives with.

---

## Settings

Stored as JSON the user can hand-edit. The settings window shows the path.

| Platform | Location |
| --- | --- |
| macOS | `~/Library/Application Support/rustafari/settings.json` |
| Linux | `$XDG_CONFIG_HOME/rustafari/`, else `~/.config/rustafari/` |
| Windows | `%APPDATA%\rustafari\` |

```json
{ "version": 1, "theme": "system", "ui_scale": 1.0,
  "font_size": 14.0, "wrap": true, "selected_tool": null }
```

Rules the implementation deliberately follows — keep them if you touch this:

- Every field is `#[serde(default)]`, so files from older **and newer** builds
  load. Unknown keys ignored, missing keys defaulted.
- Values are clamped on load. A hand-edited `"ui_scale": 100` must not produce an
  unusable window.
- A corrupt file falls back to defaults and logs. **Never refuse to start** —
  losing preferences is annoying, refusing to launch is worse.
- Writes go through a temp file + rename, so an interrupted save cannot truncate.
- Settings are saved **as they change**, not only in `on_exit`. A crash or kill
  signal never reaches `on_exit`; this was found by testing, not by reasoning.
- `version` exists for future migrations. Adding a field does **not** need a bump.

`eframe` still owns window geometry via its own persistence feature. Everything
else is ours.

---

## Look and feel

The owner asked for "more modern, more sleek, better color scheme, with icons".

- **`theme.rs`** defines a `Palette` of named roles — `base`, `surface`,
  `elevated`, `border`, three text weights, `accent`, `danger`. Dark and light are
  the same role set, so they cannot drift. **No literal colour belongs anywhere
  else in the UI**; read `p.surface`, never `Color32::from_rgb(...)`.
- Dark: near-black slate `#0E1014`, violet-indigo accent `#7C6BF5`.
  Light: `#F7F8FA` on white, deeper violet for contrast.
- Flat surfaces, hairline borders, 8px rounding. egui's stock bevels and
  hover-expansion are switched off — expansion causes rows to jitter.
- `theme::hairline(color)` is the standard 1px border. It also pins the width to
  `f32`, which `Stroke::new` cannot infer from a bare literal (this otherwise
  produces a future-incompatibility warning).

### Icons

Lucide (ISC — `assets/LUCIDE-LICENSE` must ship with any redistribution).
Codepoints live in the Private Use Area, so the font is registered as a
**fallback on every family** and an icon is just a `&str` usable in any label.

To add one: find its codepoint in
<https://unpkg.com/lucide-static@latest/font/info.json>, add a `pub const` to
`icons.rs`, then run `./scripts/subset-icons.sh`. The script reads the codepoints
out of `icons.rs` itself, so that file is the single source of truth.

---

## Release process

**Versioning:** the workspace version in the root `Cargo.toml` drives everything.
**`rustafari` 0.1.0 on crates.io is a burned version** — it was the original
hello-world stub. Published versions are immutable and can only be yanked, so the
first real release was **0.2.0**. Never try to republish an existing version.

Everything is triggered by a tag:

```sh
git tag v0.2.0 && git push --tags
```

which runs `release.yml`: builds the universal macOS DMG, Windows zip and Linux
tarball → publishes a GitHub release with `SHA256SUMS` → rewrites the Homebrew
tap's cask with the real DMG checksum → publishes `rustafari-core` then
`rustafari` to crates.io (that order is mandatory; the binary crate cannot
package before the library is live).

### Secrets

| Secret | Purpose | Status |
| --- | --- | --- |
| `CARGO_REGISTRY_TOKEN` | crates.io publish | set |
| `HOMEBREW_TAP_DEPLOY_KEY` | SSH deploy key, write access to the tap **only** | set |
| `MACOS_SIGN_IDENTITY`, `MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | signing + notarization | **not set** |

A deploy key is used rather than a PAT deliberately: a PAT would carry write
rights across every repo the account can reach.

### The Homebrew tap

Separate repo: <https://github.com/David-Portillo/homebrew-rustafari> (public).
`packaging/homebrew/rustafari.rb` in *this* repo is canonical; CI copies it over
with the version and checksum substituted. Editing the tap by hand is only for
recovering a failed release.

---

## Known state and open items

- **The DMG is unsigned.** This is the biggest gap. `brew install --cask` works,
  but a first-time user on another Mac hits a Gatekeeper block. Fixing it needs an
  Apple Developer account ($99/yr) and the seven secrets above; the workflow is
  already wired to use them the moment they exist.
- **No app icon.** `packaging/macos/rustafari.icns` does not exist; the bundler
  warns and ships the generic icon.
- **Linux gets a bare tarball**, not an AppImage or `.deb`.
- **Tool options reset each launch.** Persisting them was explicitly deferred; the
  settings format is versioned and default-tolerant so adding `tool_options` later
  is non-breaking.
- **The current UI was written without visual review** (see below). Spacing,
  contrast and alignment are unverified.

---

## Working norms in this repo

**Verify by running, not by reasoning.** Every significant bug found here was
found by executing something, and every one of them looked fine in code review:

- The release workflow was structurally invalid — the `secrets` context is **not
  available in step-level `if`** conditions. GitHub rejected the file before any
  job started.
- `gh secret set` reads its value from stdin and stores an **empty string** in a
  non-TTY shell instead of prompting. A silent-skip guard then reported a green
  release that had published nothing. Guards on a tagged release now fail loudly.
- Release assets 404 for everyone while the repo is private, even though the asset
  looks perfectly fine through the authenticated API.
- `depends_on macos: ">= :big_sur"` is a deprecated string form; use the symbol.

So: run the app, run `brew install`, run `cargo install` — don't just build.

**Screenshots do not work from this environment.** `screencapture` is denied
macOS Screen Recording permission on behalf of the terminal (Ghostty). Either the
user grants it in System Settings → Privacy & Security → Screen & System Audio
Recording and relaunches, or they capture with ⌘⇧4 + Space and the agent reads the
PNG from `~/Desktop`. **Agents can read image files** — ask for one rather than
guessing at visual results.

**Git and PRs.** Do not commit or push unless asked. Work on a branch, never
commit directly to `main`, and open a PR. Commit messages: imperative summary,
then prose explaining *why*, wrapped at 72 characters, ending with the
`Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` trailer. State what is
unverified in the PR body rather than implying everything was checked.

**Before pushing:**

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings   # CI enforces this
cargo test --workspace
actionlint .github/workflows/*.yml                      # if workflows changed
```

Linux builds need GUI headers: `libgtk-3-dev libxcb-render0-dev
libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev`.

**Local gotcha:** `cargo` on this machine is Homebrew's and has std for the host
target only, and it picks up `rustc` from `PATH`. Cross-compiling needs rustup's
pair — `scripts/bundle-macos.sh` resolves both with `rustup which` for exactly
this reason.

**Comments** explain *why*, not *what*. Match the density of the surrounding
code; it is moderately commented, and every non-obvious decision carries a
one-line rationale.
