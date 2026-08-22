# rustafari

You know the routine. You've got a blob of JSON that came out of a log line, so
you search for "json formatter", click the first result, paste your data into
someone's website, and hope it wasn't the half of the payload with the auth
token in it.

rustafari is that pile of websites as one small desktop app. JSON formatting,
diffs, Base64, hashes, UUIDs, URL encoding, cron schedules, list comparison.
Nothing you paste ever leaves your machine, because there is no network code in
the app at all.

It's also deliberately tiny. No Chromium, no webview, no bundled browser, no
JavaScript anywhere. It's Rust with an [egui](https://github.com/emilk/egui)
interface compiled to a single binary of about 2.4 MB per architecture. Starts
instantly, sits at 0% CPU when you're not touching it.

Runs on macOS, Windows and Linux from the same code.

## Install

```sh
brew tap David-Portillo/rustafari
brew install --cask rustafari
```

Heads up on macOS: the DMG isn't signed or notarized yet, so Gatekeeper will
refuse to open it on a machine that didn't build it. Right click the app and
pick Open, or run `xattr -dr com.apple.quarantine /Applications/rustafari.app`.
Getting this properly signed needs a paid Apple Developer account, and it's the
biggest rough edge in the project right now.

Windows and Linux builds are attached to every
[release](https://github.com/David-Portillo/rustafari/releases). Linux gets a
plain tarball for now, no AppImage or .deb.

If you'd rather build it yourself:

```sh
cargo run --release -p rustafari
```

`cargo install rustafari` works too, though it's a GUI app, not a command line
tool, so you get a window either way.

## The tools

| Tool | What it's for |
| --- | --- |
| JSON Formatter | Pretty-print, minify, sort keys, and tell you where the syntax error is |
| JSON Diff | Compare two documents by structure, so key order stops mattering |
| YAML Diff | Same idea, ignoring quoting, comments, anchors and merge keys |
| XML Diff | Same idea, ignoring attribute order and whitespace |
| Base64 | Encode and decode, URL-safe alphabet, padding optional |
| URL Encoder | Percent-encode and decode |
| Hash Generator | MD5, SHA-1, SHA-256, SHA-512 |
| UUID Generator | v4 random and v7 time-ordered, in bulk |
| Cron Builder | Build a schedule field by field and see the next five runs |
| List Compare | What's only in list A, only in B, or in both |

Two notes on the ones that surprise people.

The three diffs share one comparison engine and differ only in how they parse
their format. That's on purpose: three diff tools that could disagree about what
counts as a difference would be worse than one.

Cron Builder is UTC only, and says so in the heading. If you're behind UTC, the
next run it previews can show tomorrow's date, which is correct and looks wrong.
It also follows Vixie cron's rule that when you restrict both day-of-month and
day-of-week, a day matches if **either** matches. That catches everyone out, so
the tool says it out loud when your schedule hits that case.

## How it works

Input on one side, output on the other. Side by side when the window is wide
enough, stacked when it isn't, and you can drag the divider or pin either
arrangement in settings. Comparison tools take two documents and give you a pane
for each.

Output updates as you type. Tools run on a background thread, so a five megabyte
paste doesn't freeze the window.

Each pane carries its own options in a strip under its title, as icons you can
hover for the full name. Options that affect the answer sit with the input, and
options that affect how the answer is printed sit with the output. Narrow the
window and they wrap, then fold into a single button if things get really tight.

JSON, YAML and XML get syntax highlighting, with brackets coloured by nesting
depth so you can see which one closes what. The output pane also numbers its
lines and lets you fold indented blocks away.

**Send to** hands the current output straight to another tool. Decode a Base64
payload, send the result to the JSON formatter, send that to the diff. The menu
only lists tools that can actually accept what you're holding, so you won't be
offered "send this list of names to the JSON formatter". Base64 is the fun case:
when you're encoding it knows the result is Base64 and hides the JSON tools, and
when you're decoding it has no idea what came out, so it offers everything.

| Shortcut | |
| --- | --- |
| <kbd>⌘ K</kbd> / <kbd>Ctrl K</kbd> | Jump to the tool search |
| <kbd>⌘ ,</kbd> / <kbd>Ctrl ,</kbd> | Settings |
| <kbd>Esc</kbd> | Close settings, or clear the search |

## Settings

The gear in the sidebar. Theme, pane layout, interface scale, editor font size,
line wrapping, line numbers and folding, syntax highlighting.

It's a JSON file you can edit by hand, and the settings window shows you where
it lives:

| Platform | Where |
| --- | --- |
| macOS | `~/Library/Application Support/rustafari/settings.json` |
| Linux | `$XDG_CONFIG_HOME/rustafari/settings.json`, or `~/.config/rustafari/` |
| Windows | `%APPDATA%\rustafari\settings.json` |

```json
{
  "version": 1,
  "theme": "system",
  "ui_scale": 1.0,
  "font_size": 13.0,
  "wrap": true,
  "line_numbers": true,
  "syntax_highlighting": true,
  "layout": "auto",
  "pane_split": 0.5,
  "selected_tool": null
}
```

Every key is optional. Missing keys get their default, unknown keys are ignored,
silly numbers get clamped, and a file that won't parse falls back to defaults
instead of stopping the app from opening. Losing your preferences is annoying,
refusing to launch is worse. A file written by an older or newer build always
loads. Saves go through a temp file and a rename, so a save interrupted halfway
can't leave you with a truncated file.

One thing that isn't saved yet: the options you set on a tool reset when you
restart. That's on the list.

## The code

```
crates/
  rustafari-core/   All the tool logic. No UI dependency, heavily unit tested.
    src/spec.rs     The Tool trait and the option model. Read this one first.
    src/tools/      One file per tool.
  rustafari-app/    The egui desktop shell.
    src/app.rs      The interface: sidebar, panes, options, settings window.
    src/syntax.rs   Syntax highlighting, as plain lexers with no colour in them.
    src/worker.rs   Runs tools off the UI thread.
    src/settings.rs Settings and their on-disk format.
packaging/          Info.plist, Homebrew cask.
scripts/            Release bundling, font subsetting, a UI probe.
```

The core crate has zero UI dependencies and needs to keep it that way, so it
stays usable from a CLI or a test harness.

### Adding a tool

Tools declare their options and never draw them, so the interface builds itself
from whatever a tool says it needs. If your tool fits the existing input modes
and option kinds, there's no UI code to write:

1. Add `crates/rustafari-core/src/tools/<name>.rs` with a unit struct
   implementing `Tool`. You need `meta()` for the name and search terms,
   `options()` for the knobs, and `run()` for the actual work.
2. Register it in `tools/mod.rs` and in `all_tools()` in `lib.rs`.

That's it. Workspace tests then check it for you: ids are unique, every option
default names a real choice, and opening the tool with an empty input is never
an error, since that's the first thing anyone sees.

Two things to know before you write `run()`. It happens on a background thread,
so it can take as long as it needs, but it must not panic, because a panic kills
the worker and every later run quietly does nothing. Return an error instead,
and write errors as something a person would want to read, with a position in
the input if you have one.

If you're extending the app itself, `CLAUDE.md` is the long version: why the
architecture is the way it is, which decisions are settled, and a list of the
bugs that have already been found the hard way so you don't rediscover them.

## Releasing

```sh
git tag v0.7.0 && git push --tags
```

CI builds the universal macOS DMG, the Windows zip and the Linux tarball,
publishes them with a `SHA256SUMS` file, updates the
[Homebrew tap](https://github.com/David-Portillo/homebrew-rustafari) with the
new checksum, then pushes both crates to crates.io.

Two repository secrets gate the last steps, and the release fails loudly if
they're missing rather than pretending it worked:

| Secret | For |
| --- | --- |
| `HOMEBREW_TAP_DEPLOY_KEY` | SSH deploy key, write access to the tap repo only |
| `CARGO_REGISTRY_TOKEN` | Publishing to crates.io |

Signing is opt-in through `MACOS_SIGN_IDENTITY`, `MACOS_CERTIFICATE`,
`APPLE_ID`, `APPLE_TEAM_ID` and `APPLE_APP_PASSWORD`. They aren't set, so
releases currently ship an unsigned DMG.

For a local bundle:

```sh
./scripts/bundle-macos.sh   # target/dist/rustafari.app and a .dmg
```

Worth knowing: `main` is usually ahead of the latest tag, so a feature being in
the repo doesn't mean it's in the build you installed. Check with
`git describe --tags --abbrev=0`.

## Known rough edges

- The macOS DMG is unsigned, so Gatekeeper blocks it on other machines.
- No app icon yet, so it ships with the generic one.
- Linux gets a tarball, not an AppImage or a .deb.
- Tool options don't persist across restarts.
- Very large documents are slow to type into. egui re-lays-out the whole text
  on every keystroke, and fixing that properly needs a virtualized text view.

## License

MIT OR Apache-2.0
