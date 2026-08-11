# parser-rs

> 📜 Extract meaningful content from the chaos of a web page.

A Rust port of the [Postlight Mercury Parser](https://github.com/postlight/parser)
(`@postlight/parser`, v2.2.3). It fetches a web page and extracts the article:
`title`, `content`, `author`, `date_published`, `lead_image_url`, `dek`,
`next_page_url`, `url`, `domain`, `excerpt`, `word_count`, `direction`,
`total_pages`, and `rendered_pages` — with per-site custom extractors
("connectors") for ~150 known publishers, plus a generic extractor for
everything else.

## Workspace layout

| Path | Crate | Purpose |
| --- | --- | --- |
| `crates/postlight-parser` | `postlight-parser` | Core library (async, tokio-based) — usable from a Tauri app |
| `crates/postlight-parser-cli` | `postlight-parser-cli` | `mercury-parser`-style CLI for testing and scripting |

## Prerequisites

- **Rust (stable)** — install with [rustup](https://rustup.rs). The project
  uses edition 2021 and tracks current stable Rust.
- **Linux only** — OpenSSL development headers. The HTTP client uses
  `reqwest`'s default `native-tls` backend:
  `sudo apt install libssl-dev` (Debian/Ubuntu) or
  `sudo dnf install openssl-devel` (Fedora).
  Windows (schannel) and macOS (Security.framework) need no extra system
  packages.

## Build

```bash
git clone https://github.com/rahuldshetty/pl-paser-rs.git
cd parser-rs

# Debug build (library + CLI)
cargo build

# Optimized release binaries
cargo build --release

# Optional: enable the `fallback` feature (readability-based last-resort
# content extraction) for the library
cargo build -p postlight-parser --features fallback
```

## Install the CLI

```bash
cargo install --path crates/postlight-parser-cli
```

installs the `postlight-parser` binary into `~/.cargo/bin`
(`%USERPROFILE%\.cargo\bin` on Windows). Run `postlight-parser --help` for all
options. Without installing, you can run it directly from the workspace with
`cargo run -p postlight-parser-cli -- <url>`.

## Test

```bash
cargo test --workspace
```

Fixture tests run fully offline. The single `#[ignore]`d test in
`crates/postlight-parser/src/resource.rs` hits a live network endpoint; run it
with `cargo test --workspace -- --ignored`.

## Usage (library)

```rust,ignore
use postlight_parser::{ContentType, ParseOptions, Parser};

#[tokio::main]
async fn main() -> Result<(), postlight_parser::ParserError> {
    let mut opts = ParseOptions::default();
    opts.content_type = ContentType::Markdown;

    let article = Parser::parse("https://en.wikipedia.org/wiki/Thunder_(mascot)", &opts).await?;
    println!("{}", serde_json::to_string_pretty(&article)?);
    Ok(())
}
```

In a Tauri app, call `Parser::parse` from a `#[tauri::command]`:

```rust,ignore
#[tauri::command]
async fn extract(url: String) -> Result<postlight_parser::Article, String> {
    postlight_parser::Parser::parse(&url, &postlight_parser::ParseOptions::default())
        .await
        .map_err(|e| e.to_string())
}
```

### Options

`ParseOptions` mirrors upstream `parse(url, opts)`:

- `html` — pre-fetched HTML to parse instead of fetching `url`
  (`Parser::parse_html` does the same synchronously-ish without a fetch).
- `fetch_all_pages` (default `true`) — follow `next_page_url` chains and
  merge content (`<hr><h4>Page N</h4>` separators).
- `fallback` (default `true`) — fall back to the generic extractor when a
  custom selector misses.
- `content_type` (default `html`) — `ContentType::Html | Markdown | Text`.
- `headers` — extra request headers as `(name, value)` pairs.
- `extend` — extra output fields by CSS selector.
- `custom_extractor` — register a `CustomExtractor` at runtime.

### Custom extractors

~150 built-in extractors are ported from upstream, plus the transform
functions they rely on. `postlight_parser::extractors::custom::add_extractor`
registers a runtime extractor; the selector model mirrors the upstream
schema (`selectors`, `[selector, attr]` pairs, content multi-match arrays,
`clean`, `transforms`, `date_published.format`/`timezone`,
`defaultCleaner`, `extend`).

Regenerating the built-in table after upstream changes:

```bash
python tools/generate_custom_extractors.py \
  <path/to/postlight/parser/src/extractors/custom> \
  crates/postlight-parser/src/extractors/custom_data.rs /tmp/named.txt
```

## Usage (CLI)

```bash
# Fetch and parse a URL
postlight-parser https://en.wikipedia.org/wiki/Thunder_\(mascot\)

# Parse pre-fetched HTML; output markdown
postlight-parser --file page.html --format markdown https://example.com/article

# Custom headers, no pagination
postlight-parser --header "Cookie: a=b" --no-fetch-all-pages https://example.com/
```

## Status

Port of upstream v2.2.3: resource fetching (redirects, charset decoding,
lazy-image lifting), generic extractor (cleaning/scoring pipeline, all
metadata fields), all ~150 custom site extractors with their transforms,
next-page collection, and html/markdown/text output. Upstream has no
readability integration; the optional `fallback` cargo feature adds one as a
last-resort content extractor.

## License

MIT OR Apache-2.0 (same dual license as the upstream project).
