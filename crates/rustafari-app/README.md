# rustafari

A Swiss Army knife devtoy: a single native desktop app bundling the small
utilities you'd otherwise paste into a random website — JSON formatting,
structural JSON/YAML/XML diffs, Base64, hashing, UUIDs, URL encoding, a cron
expression builder and a list comparator.

Everything runs **locally and offline**. Nothing you paste leaves your machine.

No Chromium, no webview, no JavaScript — pure Rust with an
[`egui`](https://github.com/emilk/egui) interface, compiled to one static binary.

```sh
cargo install rustafari   # then run `rustafari`
```

Prebuilt macOS, Windows and Linux downloads, plus a Homebrew cask, are on the
[project page](https://github.com/David-Portillo/rustafari).

Building from source on Linux needs the usual GUI development headers
(`libgtk-3-dev`, `libxkbcommon-dev`, and the `libxcb-*-dev` set).

The tool logic lives in [`rustafari-core`](https://crates.io/crates/rustafari-core),
which carries no UI dependency.

## License

MIT OR Apache-2.0
