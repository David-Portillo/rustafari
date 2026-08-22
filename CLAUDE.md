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
| **Lean binaries** | 2.3 MB per architecture; the universal macOS binary is 5.0 MB and its DMG 3.3 MB. Weigh any dependency against that. Fonts are subset rather than vendored whole (854 KB of Lucide → 8 KB), and egui's bundled font set was dropped for a lighter one. |
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
    src/tools/{json,base64,url,hash,uuid,cron,list_compare}.rs
    src/tools/structural.rs     the diff engine: what "structural" means, decided once
    src/tools/{json,yaml,xml}_diff.rs  a parser each, on top of it
  rustafari-app/                 egui/eframe desktop shell
    src/main.rs                  window setup (incl. macOS titlebar), module declarations
    src/app.rs                   the UI: sidebar, header, options, panes, status bar, settings
    src/widgets.rs               custom widgets: icon_button, segment, toggle, splitter
    src/folding.rs               which output lines can fold, and what stays visible
    src/worker.rs                background thread that runs tools; coalesces + drops stale
    src/settings.rs              user settings and their on-disk format
    src/theme.rs                 palette, egui Visuals, text styles, spacing
    src/fonts.rs                 installs the bundled fonts (replaces egui's defaults)
    src/icons.rs                 icon codepoints (Lucide, Private Use Area)
    assets/Inter-Medium.ttf      UI font, subset            (136 KB)
    assets/JetBrainsMono-Regular.ttf  code font, subset     ( 75 KB)
    assets/NotoEmoji-Regular.ttf emoji fallback, whole      (419 KB)
    assets/lucide.ttf            icon font, subset          (  8 KB)
    assets/*-LICENSE             OFL / ISC attributions — must ship with the app
packaging/
  macos/Info.plist               bundle metadata; __VERSION__ is substituted at build
  homebrew/rustafari.rb          canonical cask; CI copies it to the tap
scripts/
  bundle-macos.sh                universal .app + DMG, opt-in signing/notarization
  subset-fonts.sh                regenerates Inter / JetBrains Mono / Lucide subsets
  ui-probe.sh/.swift             drives the running app with real mouse events
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

`InputMode` is `Text` (one document), `TwoText` (two, for comparisons — the
frontend shows a labelled pane per side), or `None` (generators). `run` receives
an `Input` with `left` and `right`; `right` is empty unless the tool asked for
`TwoText`, so single-input tools just read `left`.

`OptionSpec` is `Toggle` / `Choice` / `Number` / `Text` / `Group`. `Group` is a
heading that starts a new row of options, for a tool with enough knobs that it
helps to say which of them decide the answer and which decide how it reads. `Text` is a short
inline field for values no picker can enumerate — a cron field, a delimiter.
Keep it short: it sits in the options row, not in a pane. A tool built entirely
from `Text` options with `InputMode::None` is a form, which is how the cron
builder works.

The payoff: **a tool whose shape already exists needs no UI code at all.**

Read that precisely, because the obvious stronger claim is false. What the
frontend renders generically is the *vocabulary* — the three `OptionSpec` kinds
and the three `InputMode` variants. A tool built from those is pure core code.
Introducing a **new** shape is a one-time frontend change, and then every later
tool of that shape is free.

JSON Diff is the worked example: comparison needed two documents, which the
vocabulary could not express, so `InputMode::TwoText` and the two-pane layout had
to be written once. The next comparison tool costs nothing. Assume you are in
the free case; check `InputMode` and `OptionSpec` before promising it.

### Adding a tool

1. Create `crates/rustafari-core/src/tools/<name>.rs` with a unit struct
   implementing `Tool`.
2. Register it in `src/tools/mod.rs` and in `all_tools()` in `src/lib.rs`.

That is the whole procedure **when an existing `InputMode` and the existing
`OptionSpec` kinds cover the tool**. If they do not, extend `spec.rs` first and
teach `app.rs` to draw the new thing; keep that change generic, so it serves the
variant rather than the one tool that prompted it.

Three workspace-level tests in `lib.rs` then police the new tool automatically:

- ids are unique,
- every `Choice` default names a real choice,
- running with default options and empty input is never an error (that is the
  state the user sees when they open a tool).

An icon is optional: `tool_icon()` in `app.rs` maps known ids to glyphs and falls
back per category, so an unmapped tool still looks right.

`Options` is built from the specs, so every getter has a default and **cannot
fail** — tool implementations never handle missing options.

### The diff family

`structural.rs` owns the comparison; `json_diff`, `yaml_diff` and `xml_diff`
each supply only a parser into `serde_json::Value` and share the engine's three
options. Keep it that way — the value of three diff tools is that they cannot
disagree about what a difference is.

The mapping each format uses *is* its definition of "semantic", so it belongs in
that tool's module docs, and the interesting cases belong in its tests:

- **YAML** drops comments, quoting and block/flow style at the parser. Anchors,
  aliases and `<<` merge keys are resolved first — `apply_merge` is a separate
  call and skipping it makes every merged block read as a `<<` key, which is how
  it was wrong the first time.
- **XML** maps elements to arrays under their tag name (always arrays, so a
  second `<item>` reads as an addition rather than a type change), attributes to
  `@name`, text to `#text` with whitespace collapsed, and namespaced names to
  Clark notation `{uri}local`. Comments and declarations are dropped.

### Cron

`cron.rs` computes dates itself rather than depending on `chrono`, which is only
affordable because the tool is **UTC-only** — no time zones and no daylight
saving, so the arithmetic is exact civil-calendar maths (Hinnant's algorithms)
in about thirty lines. Do not add local-time support without accepting that
dependency and its timezone database.

Two behaviours that look like bugs and are not:

- When both day-of-month and day-of-week are restricted, a day matches if
  **either** does. That is Vixie cron's rule, it surprises everyone, and the
  description says so out loud when it applies.
- Being behind UTC means the previewed next run can carry tomorrow's date.
  The heading says `(UTC)` for exactly this reason; do not quietly drop it.

`report()` takes "now" as a parameter so the tests can pin a timestamp and
assert exact dates. Keep it that way — a schedule tool that can only be tested
against the real clock cannot be tested.

### Conventions inside tools

- Errors are user-facing copy, not debug dumps. `ToolError::new("Not valid
  Base64: …")`, and include position information when the parser offers it.
- Empty input returns `Ok("")` for transforms. Hashes are the exception — the
  digest of the empty string is a real answer.
- Be forgiving about input shape where it costs nothing: Base64 decoding strips
  the whitespace that line-wrapped input arrives with.

### `run()` executes on a background thread

Since the performance pass, tools no longer run on the UI thread (see
`worker.rs`). Consequences for anything new:

- **`run()` may take as long as it needs** without freezing the interface. It is
  still called on every keystroke, so it should stay reasonable, but tens of
  milliseconds is fine — the worker coalesces bursts.
- **Do not panic.** A panic kills the worker thread. The app survives and keeps
  showing the last good output, but every subsequent run silently does nothing.
  Return `Err` instead.
- `Tool` is `Send + Sync` and tools are held in `Arc`, so **no interior
  mutability without synchronisation**. Every tool so far is a stateless unit
  struct; keep it that way unless there is a real reason not to.
- `run()` takes `&self`, so a tool cannot cache into itself anyway.

---

## Settings

Stored as JSON the user can hand-edit. The settings window shows the path.

| Platform | Location |
| --- | --- |
| macOS | `~/Library/Application Support/rustafari/settings.json` |
| Linux | `$XDG_CONFIG_HOME/rustafari/`, else `~/.config/rustafari/` |
| Windows | `%APPDATA%\rustafari\` |

```json
{ "version": 1, "theme": "system", "ui_scale": 1.0, "font_size": 13.0,
  "wrap": true, "layout": "auto", "pane_split": 0.5, "selected_tool": null }
```

`layout` is `auto` / `side-by-side` / `stacked`; `pane_split` is the input's
share of the pane area, set by dragging the divider and saved on release.

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
- **A pane is one box: title, rule, toolbar, rule, content** (`pane_box` in
  `app.rs`). The options belong *inside* the box they drive, because with two
  inputs and an output on screen a row of controls floating above a box is
  ambiguous about which box it belongs to. Options sit at the left of the
  toolbar drawn icon-first with the label as a tooltip; actions hug the right.
- **A toolbar's option row must reserve width for the actions beside it.**
  egui lays the actions out right-to-left and, on overflow, draws them *over*
  the options rather than clipping — so `pane_options` takes a `reserve` and
  collapses the whole group into a gear-button popup when it will not fit.
  The width it is handed already excludes the title, and subtracting an
  allowance for the title on top of that made every header collapse even at
  1512 px. Measure with a temporary `eprintln!`, do not reason about it.
- **Running out of width wraps the options; it does not hide them.** The row
  packs onto as many lines as it needs, up to `MAX_OPTION_ROWS`, and only past
  that falls back to the gear. Collapsing a pane that had room to show its
  controls reads as the options being missing.
- **The rows are packed by hand, not by `horizontal_wrapped`.** Each option is
  a nested `horizontal` whose width egui does not know until it has placed it,
  so a wrapped layout cannot wrap it — it silently overflowed into the
  neighbouring pane instead. `pack_options` decides the line breaks from
  `option_width`, which is a deliberate *over*-estimate: a row left slightly
  short is invisible, a row overrun is not.
- **The second pane of a comparison reserves its row by drawing the real
  options invisibly** (`Ui::set_invisible`), not by computing a height from a
  constant. Two constants that must agree with a layout drift: the computed
  version sat five pixels off and the two editors visibly failed to line up.
- **The collapsed options panel is a hand-rolled `Area`, not an egui popup.**
  egui's memory holds exactly one open popup id, so a `ComboBox` opened inside
  a popup *evicts the popup containing it* — the panel vanished the instant a
  dropdown in it was clicked, and the choice could never be made. The panel
  therefore keeps its own open flag in `Ui::data` and dismisses itself on
  Escape, on the gear, or on a click outside — but only while
  `any_popup_open()` is false, since a click landing in a dropdown of its own
  belongs to that dropdown.
- **The title row carries the actions; the toolbar row carries only options.**
  `Clear`, `Copy`, `Send to` and `Generate` are labelled buttons beside the
  pane's name — an unlabelled `✕` beside a text box does not say what it
  clears, and these are the destructive and the outward-facing ones.
- **An action is disabled, never hidden, when there is nothing to act on.**
  Deriving a control's presence from content made the row appear with the
  first character typed, moving the editor down mid-keystroke. The toolbar
  row's presence is allowed to vary only with the *tool*, which never changes
  under the user: it is drawn when the tool declares options for that pane.
- **A comparison's second pane keeps an empty toolbar row.** The options
  belong to the first pane only, but side by side, two boxes whose editors
  start at different heights read as a bug.
- **A `Choice` with more than four options renders as a dropdown**, fewer as a
  segmented control. Segments read faster but only while they fit; List Compare
  declares nine options, three of them with five to eight choices, and as
  segments they ran off the edge of the window.
- **A label travels with the control it names.** Each option is allocated as a
  single unit, so a wrapped row breaks between options instead of stranding
  "Output case" at the end of one line and its buttons at the start of the next.
- **A widget that wants to wrap has to measure itself before it claims space.**
  `segmented` originally drew inside a `Frame`, so the row only learned its
  width after placing it — a wrapped layout cannot wrap what it has not
  measured, and it overflowed instead of moving to the next line.
- Custom widgets live in `widgets.rs` as plain functions taking the palette:
  `icon_button`, `segment` (segmented controls replace dropdowns), `toggle`
  (an animated switch — egui's checkbox can't fill with the accent when on),
  `splitter` (the draggable pane divider), and `slider`.
- **Add a widget through `widgets.rs`, not at the call site.** `slider` exists
  because egui paints a slider's rail in `widgets.inactive.bg_fill`, which this
  palette sets to `elevated` — the colour of the settings window the sliders sit
  in, so the track was invisible. Fixing the two call sites would have left the
  third one to rediscover it.
- **Two palette roles resolving to the same colour on the same surface is this
  project's recurring UI bug.** It has produced an invisible window title and an
  invisible slider track. `theme.rs` now carries contrast tests over the palette
  itself, which is testable where the rendering is not; extend them rather than
  fixing instances.
- **Layout is responsive.** Input and output sit side by side when the central
  area is ≥ 860 px wide, stacked otherwise (`PaneLayout::Auto`; users can pin
  either). The divider between them drags. Generators (no input) get the whole
  area. Layout uses explicit rects, not egui's flow layout, so both panes fill
  the space exactly.
- On macOS the content extends under a transparent title bar
  (`with_fullsize_content_view` + `with_titlebar_shown(false)`), so
  `TITLEBAR_INSET` pads the sidebar and central panel clear of the traffic
  lights. It is 0 elsewhere. **`with_titlebar_shown(false)` is misleadingly
  named** — in egui 0.29 it maps to winit's `with_titlebar_transparent`, so the
  bar goes transparent and the traffic lights stay. There is no
  `with_titlebar_transparent` on `ViewportBuilder`; reaching for it is a compile
  error.
- Shortcuts: ⌘K / Ctrl+K focuses search, ⌘, / Ctrl+, toggles settings, Esc
  closes settings or clears the search.

<<<<<<< HEAD
### List Compare

Every delimiter mode also breaks on line endings. Choosing "Commas" on a
multi-line paste otherwise yields one item per line with commas inside it, and
choosing a delimiter the data does not contain yields a single item holding the
whole list — which looks exactly like sorting and comparison being broken.

The rule worth preserving: **normalisation decides what counts as the same
item, and never changes what is printed.** Trimming, case folding, collapsing
spaces and ignoring leading zeros all build a comparison key; the output is the
item as it was first written. Comparing case-insensitively still shows `Alice`,
not `alice`. The output transforms (case, sort, format) apply afterwards, to the
display text only.
=======
### Line numbers and folding

The output pane numbers its lines and lets indented blocks be folded, behind the
`line_numbers` setting. Two things make this cheap:

- **Folding is decided by indentation, not by syntax.** Every format the app
  produces indents its nesting — JSON, XML, YAML, the diff reports, the cron
  preview — so one rule covers all of them and a tool added later gets folding
  without anyone thinking about it. `folding.rs` owns that rule and is tested on
  its own.
- **The output is read-only**, so folding can simply *display a different
  string*. Nothing rewrites the real output: `Copy` and `Send to` always take
  the full text from `self.output`.

Line numbers are positioned from the galley's rows rather than by multiplying a
row height, so a wrapped line still gets exactly one number against its first
visual row. The gutter is reserved inside a `horizontal_top` — in a vertical
layout a zero-height allocation reserves a row rather than a column, and the
text lands underneath the numbers.

Fold state is cleared whenever the output changes, since the line numbers it
refers to would no longer mean the same thing.
>>>>>>> origin/main

### Chaining

The output pane's **Send to** menu hands the current output to another tool as
its input. That is the deliberately cheap two thirds of CyberChef's recipe
model: the everyday chain — decode this, now pretty-print it — is two links, and
two links need no recipe format, no stage rules and no second navigation model.

**The menu only lists destinations that can actually work.** `ToolMeta` is not
enough for that, so `Tool` declares two more things: `accepts()` — the
`Format`s it can be handed — and `produces(&Options)`, which takes the options
because for several tools the answer depends on them. `Format` is deliberately
coarse (`Any`, `Plain`, `Json`, `Yaml`, `Xml`, `Base64`); it exists to keep
"send my list of names to the JSON formatter" out of the menu, not to
type-check a pipeline.

The rule that makes it usable is `Format::Any`, which means *no claim made*.
Base64 **decode** produces `Any` because it genuinely does not know what came
out, so decode-then-pretty-print — the chain the feature exists for — survives
the filter, while Base64 **encode**, which is known to produce Base64, does
not offer the JSON tools. Only a format that is known *and* wrong is refused.
Measured in the running app: Base64 offers 3 destinations when encoding and 7
when decoding.

It works only because `Tool::run` has always been a pure function from text to
text. The menu lists only real destinations: generators have nowhere to put the
text, and the tool you are already in is not somewhere to go. Comparison tools
receive it as the left-hand document.

If a real pipeline is ever wanted, the thing to weigh is not the plumbing but
the tool set: the diffs take two inputs and the generators take none, so a
linear chain does not fit them without rules that tax every future tool.

### Fonts

egui's `default_fonts` feature is **off**; `fonts.rs` installs our own. This
was a net −760 KB on the binary (3.10 → 2.34 MB) *and* the single biggest visual
upgrade, since egui's stock Ubuntu-Light is what makes egui apps look like egui
apps.

| Family | Font | Note |
| --- | --- | --- |
| Proportional | Inter **Medium** | Medium, not Regular — egui's rasterizer is unhinted and Regular reads thin at 13 px. Same call Rerun made. |
| Monospace | JetBrains Mono Regular | The panes. |
| fallback | Noto Emoji | Vendored whole from egui's set so emoji in pasted JSON render. |
| fallback | Lucide (subset) | Icons. |

Inter and JetBrains Mono are subset to Latin + Latin Extended + Greek + Cyrillic
— the coverage egui's defaults had. Anything outside (CJK, Arabic…) is tofu, as
it was before. `./scripts/subset-fonts.sh` regenerates all three subsets from
their upstream sources.

### Icons

Lucide (ISC — `assets/LUCIDE-LICENSE` must ship with any redistribution).
Codepoints live in the Private Use Area, so the font is registered as a
**fallback on every family** and an icon is just a `&str` usable in any label.

To add one: find its codepoint in
<https://unpkg.com/lucide-static@latest/font/info.json>, add a `pub const` to
`icons.rs`, then run `./scripts/subset-fonts.sh`. The script reads the codepoints
out of `icons.rs` itself, so that file is the single source of truth.

---

## Performance model

Measured, not assumed. Keep these properties:

- **Tools run on a background thread** (`worker.rs`). On a 5 MB JSON paste the
  formatter takes ~70 ms; on the UI thread that dropped four frames per
  keystroke. The worker is one long-lived thread that always skips to the newest
  queued job, so a burst of keystrokes costs one run, and results carrying a
  stale generation are dropped. `Tool: Send + Sync` exists for exactly this.
  Submissions are coalesced to one per frame via the `dirty` flag.
- **Idle CPU is 0%.** Nothing calls `request_repaint` unconditionally. The
  worker wakes the renderer via `on_done`; the "Working…" indicator and the
  "Copied" revert use `request_repaint_after` with a deadline, not a spin.
- **No per-frame O(n) work on text.** Character/line counts (`TextStats`) are
  computed on change. The sidebar filter is recomputed on query change, not per
  frame. `apply_settings` compares only style-relevant inputs, so dragging the
  divider doesn't rebuild the style. `segment` lays its label out once, not
  twice.
- **Known limit:** egui's `TextEdit` re-lays-out its whole text when it
  changes, so a multi-megabyte *input* still costs on each keystroke, and a huge
  *output* costs on first display. That is an egui limitation; a virtualized
  viewer would be the fix. Not addressed.

### Re-measuring

Do this before claiming any optimisation, and after. The numbers above came from:

- **Tool cost:** a throwaway crate depending on `rustafari-core` by path, which
  builds a ~5 MB JSON document and times `tool.run()` for every tool in
  `all_tools()`. Reference figures on an M-series Mac, release build: JSON
  formatter 71 ms, URL encoder 15 ms, hash 14 ms, Base64 2 ms, UUID <1 ms.
- **Idle CPU:** launch the release binary, leave it alone, then sample
  `ps -o %cpu= -p $(pgrep -f target/release/rustafari)` once a second. Must be
  0.0. Anything else means something is repainting unconditionally.
- **Binary size:** `ls -l target/release/rustafari`, and
  `./scripts/bundle-macos.sh` for the universal + DMG figures.
- **Resize behaviour:** drive it from AppleScript rather than by hand —
  `osascript -e 'tell application "System Events" to tell process "rustafari"
  to set size of window 1 to {700, 500}'` — through the breakpoints either side
  of `SIDE_BY_SIDE_MIN_WIDTH` and down to the 680×460 minimum.

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

- **`main` is usually ahead of the last release.** What users have from
  `brew install` or crates.io is the newest tag, not `main`. Check with
  `git describe --tags --abbrev=0` and `git log --oneline $(git describe --tags
  --abbrev=0)..main` before telling anyone a feature is available. Shipping is
  one tag push — see the release process above.
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
- **The UI is only partly reviewed.** Dark theme, both pane layouts, the
  settings window and the sidebar have now been seen and driven (see the probe
  below); the macOS titlebar inset is confirmed clear of the traffic lights.
  The light theme has now been seen too — and immediately produced a bug, a
  window title that rendered white on white. **Not yet looked at: the error
  banner, and the tools at anything but default options.** Expect the same
  class of bug there that every reviewed surface has had so far.

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

**Measure before optimising.** The performance pass started by timing the tools
and sampling idle CPU, which is how the actual problem (a 71 ms run on the UI
thread, on every keystroke) surfaced. Two bugs in the same pass were only
findable by thinking in real units:

- "Copied" reverted after a fixed number of *frames*, which is a different
  duration on a 60 Hz and a 120 Hz display. Timed UI state belongs in `Instant`
  + `request_repaint_after`, never a frame counter.
- No-wrap mode passed an infinite *desired width* to `TextEdit`, which makes it
  allocate infinite space and breaks the scroll bars. Disabling wrap in egui
  means a custom layouter with an infinite wrap width inside a two-axis
  `ScrollArea` — the width the widget asks for and the width it wraps at are
  different things.

**Prefer widening an existing test over adding a parallel one.** The clamp test
in `settings.rs` grew a `pane_split` case rather than gaining a sibling.

**Look at the UI, and drive it.** `./scripts/ui-probe.sh` exists because two
interaction bugs — a scale slider that could not be aimed, and tool names that
ignored clicks — shipped past every test, lint and screenshot we have. Neither
was visible in a still image, and neither was reachable by AppleScript, because
the app is built without accesskit and so answers no accessibility API. The
probe posts real CGEvents instead:

```sh
./scripts/ui-probe.sh raise             # bring the app forward
./scripts/ui-probe.sh shot out.png      # screenshot, this app's window only
./scripts/ui-probe.sh click X Y         # a real click
./scripts/ui-probe.sh drag X1 Y1 X2 Y2  # a real press-move-release
```

Read coordinates off a `shot`, remembering a Retina capture has twice the pixels
of the points you click in — divide by the ratio between the image width and the
window width that `window` reports. Verify the *effect*, not just the pixels:
`~/Library/Application Support/rustafari/settings.json` shows which tool got
selected and where a slider landed.

`shot` captures **by window id, never by screen region**. A region capture picks
up whatever else is on screen, which is none of this project's business; one
did, early on, and had to be destroyed. Do not reach for `screencapture -R`.

**Check which process the probe is driving.** It finds a window by owner name,
so an installed copy — `/Applications/rustafari.app`, or anything `brew
install --cask` put there — will be picked up instead of your build, and
`open -a rustafari` resolves to that copy by name rather than to
`target/`. This cost an hour once: screenshots showed a v0.5.0 UI with none of
the changes in them, and the obvious conclusions (stale binary, broken build)
were all wrong. Run `ps aux | grep rustafari` first, kill anything that is not
your build, and launch `./target/release/rustafari` by path.

It needs Screen Recording and Accessibility permission for whichever terminal
runs it (System Settings → Privacy & Security). **Agents can read image files**,
so if the probe is unavailable, ask the user for a PNG and read it.

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
