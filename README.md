# rustafari

A Swiss Army knife devtoy for developers: a single native desktop app bundling
the small utilities you'd otherwise paste into a random website — JSON
formatting, Base64, hashing, UUIDs, URL encoding.

Everything runs **locally and offline**. Nothing you paste leaves your machine.

- **No Chromium, no webview, no JavaScript.** Pure Rust with an
  [`egui`](https://github.com/emilk/egui) interface, compiled to one static
  binary — 2.3 MB per architecture; the universal macOS app is 5 MB, its DMG 3.3 MB.
- **macOS, Windows and Linux** from one codebase.

## Install

```sh
# macOS
brew tap David-Portillo/rustafari
brew install --cask rustafari
```

Windows and Linux builds are attached to each [release](https://github.com/David-Portillo/rustafari/releases).

### From source

```sh
cargo run --release -p rustafari
```

## Tools

| Tool | Category | What it does |
| --- | --- | --- |
| JSON Formatter | Formatters | Validate, pretty-print, minify, sort keys |
| JSON Diff | Formatters | Compare two documents structurally, ignoring key order |
| YAML Diff | Formatters | Same, ignoring quoting, comments and anchors |
| XML Diff | Formatters | Same, ignoring attribute order and whitespace |
| Base64 | Encoders | Encode/decode, URL-safe alphabet, optional padding |
| URL Encoder | Encoders | Percent-encode and decode |
| Hash Generator | Generators | MD5, SHA-1, SHA-256, SHA-512 |
| UUID Generator | Generators | v4 (random) and v7 (time-ordered), in bulk |
| Cron Builder | Generators | Build a schedule field by field, with the next runs previewed |
| List Compare | Text | What's unique to each of two lists and what they share |

## Using it

Input on one side, output on the other — side by side when the window is wide,
stacked when it isn't, with a divider you can drag. Comparison tools like JSON
Diff take two documents and show a pane for each. Output updates as you type;
tools run on a background thread, so a large paste never freezes the interface.

Output can be handed straight to another tool with **Send to** — decode a
base64 payload, send it to the JSON formatter, send that to the diff. No copying
and pasting between tools.

| Shortcut | |
| --- | --- |
| <kbd>⌘ K</kbd> / <kbd>Ctrl K</kbd> | Focus the tool search |
| <kbd>⌘ ,</kbd> / <kbd>Ctrl ,</kbd> | Settings |
| <kbd>Esc</kbd> | Close settings, or clear the search |

## Settings

The gear in the sidebar opens Settings: theme (System / Light / Dark), pane
layout (Auto / Side by side / Stacked), interface scale, editor font size, line
wrapping, and line numbers with folding.

They're stored as JSON you can read and edit by hand — the settings window shows
the path:

| Platform | Location |
| --- | --- |
| macOS | `~/Library/Application Support/rustafari/settings.json` |
| Linux | `$XDG_CONFIG_HOME/rustafari/settings.json`, else `~/.config/rustafari/` |
| Windows | `%APPDATA%\rustafari\settings.json` |

```json
{
  "version": 1,
  "theme": "system",
  "ui_scale": 1.0,
  "font_size": 13.0,
  "wrap": true,
  "line_numbers": true,
  "layout": "auto",
  "pane_split": 0.5,
  "selected_tool": null
}
```

Every key is optional. Missing keys take their default, unrecognised keys are
ignored, out-of-range numbers are clamped, and an unparseable file falls back to
defaults rather than stopping the app from starting — so a settings file from an
older or newer build always loads. Writes go through a temporary file and a
rename, so an interrupted save can't truncate your settings.

## Layout

```
crates/
  rustafari-core/   Pure tool logic. No UI dependency, fully unit tested.
    src/spec.rs     The Tool trait and the option model.
    src/tools/      One file per tool.
  rustafari-app/    egui/eframe desktop shell.
    src/settings.rs User settings and their on-disk format.
    src/worker.rs   Runs tools off the UI thread.
    src/fonts.rs    Bundled fonts (Inter, JetBrains Mono, Noto Emoji, Lucide).
packaging/          Info.plist, Homebrew cask.
scripts/            Release bundling.
```

### Adding a tool

The frontend renders whatever options a tool declares, so a tool built from the
existing input modes and option kinds needs no UI code:

1. Create `crates/rustafari-core/src/tools/<name>.rs` with a unit struct
   implementing `Tool` — `meta()` for identity and search terms, `options()` for
   the knobs, `run()` for the logic.
2. Register it in `tools/mod.rs` and in `all_tools()` in `lib.rs`.

Workspace-wide tests then check it automatically: unique id, coherent option
defaults, and that opening it with an empty input is never an error.

## Releasing

```sh
git tag v0.7.0 && git push --tags
```

CI builds the universal macOS DMG, the Windows zip and the Linux tarball,
publishes them with a `SHA256SUMS` file, updates the
[Homebrew tap](https://github.com/David-Portillo/homebrew-rustafari) with the
DMG's checksum, and pushes both crates to crates.io.

Each of those last two steps is gated on a secret and skipped when it is absent:

| Secret | Used for |
| --- | --- |
| `HOMEBREW_TAP_DEPLOY_KEY` | SSH deploy key with write access to the tap repo only |
| `CARGO_REGISTRY_TOKEN` | crates.io publish |

Signing is opt-in via repository secrets (`MACOS_SIGN_IDENTITY`,
`MACOS_CERTIFICATE`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD`). Without
them the build still succeeds but produces an **unsigned** DMG that Gatekeeper
will block on other machines — so signing and notarization are required before
the cask is usable by anyone but you.

To build a local bundle:

```sh
./scripts/bundle-macos.sh   # -> target/dist/rustafari.app + .dmg
```

## License

MIT OR Apache-2.0
