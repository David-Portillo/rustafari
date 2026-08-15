# rustafari-core

The tool logic behind [rustafari](https://github.com/David-Portillo/rustafari), a
Swiss Army knife devtoy for developers.

This crate has **no UI dependency**. Everything in it is pure and synchronous:
no network, no filesystem, no globals — so it works equally well behind the
desktop app, a CLI, or your own tooling.

```rust
use rustafari_core::{all_tools, Options};

let tools = all_tools();
let json = &tools[0];
let opts = Options::from_specs(json.options());

assert_eq!(json.run(r#"{"a":1}"#, &opts).unwrap(), "{\n  \"a\": 1\n}");
```

Tools *declare* their options via `OptionSpec` rather than drawing them, so a
frontend can render any tool generically and adding a tool needs no UI code.

Included: JSON formatter, Base64, URL encoder, hash generator (MD5/SHA-1/SHA-256/SHA-512),
and a UUID generator (v4/v7).

## License

MIT OR Apache-2.0
