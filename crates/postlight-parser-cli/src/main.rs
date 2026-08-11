//! `postlight-parser` — command-line interface for the `postlight-parser` library.
//!
//! Mirrors the upstream `mercury-parser` CLI: fetch a URL (or read HTML from
//! stdin later) and print the extracted article as JSON.
//!
//! The full implementation lands with the extraction pipeline (Phase 5c);
//! this scaffold only wires up argument parsing so the binary compiles and
//! `--version` works.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "postlight-parser",
    version,
    about = "Extract meaningful content from the chaos of a web page"
)]
struct Cli {
    /// URL of the article to parse
    url: String,
}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();

    // TODO(5c): wire up Parser::parse(url).await and print JSON.
    eprintln!("parser engine not yet wired up (scaffold)");
    Ok(())
}
