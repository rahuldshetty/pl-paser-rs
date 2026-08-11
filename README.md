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

## Status

Under construction — see the project task list for the current milestone.

## License

MIT OR Apache-2.0 (same dual license as the upstream project).
