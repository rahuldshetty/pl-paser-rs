//! `postlight-parser` — command-line interface for the `postlight-parser`
//! library, mirroring the upstream `mercury-parser` CLI.

use std::path::PathBuf;

use clap::Parser as ClapParser;
use postlight_parser::{ContentType, ParseOptions, Parser};

#[derive(ClapParser, Debug)]
#[command(
    name = "postlight-parser",
    version,
    about = "Extract meaningful content from the chaos of a web page"
)]
struct Cli {
    /// URL of the article to parse
    url: String,

    /// Output format for the content field: html, markdown, or text
    #[arg(long, default_value = "html", value_name = "FORMAT")]
    format: String,

    /// Custom request header (NAME=VALUE); may be repeated
    #[arg(long = "header", value_name = "NAME=VALUE")]
    headers: Vec<String>,

    /// Do not follow next-page chains
    #[arg(long)]
    no_fetch_all_pages: bool,

    /// Do not fall back to the generic extractor
    #[arg(long)]
    no_fallback: bool,

    /// Parse pre-fetched HTML from a file instead of fetching the URL
    #[arg(long, value_name = "FILE")]
    file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let content_type = match ContentType::parse(&cli.format) {
        Some(ct) => ct,
        None => anyhow::bail!(
            "unknown format {:?} (expected html, markdown, or text)",
            cli.format
        ),
    };

    let mut opts = ParseOptions {
        fetch_all_pages: !cli.no_fetch_all_pages,
        fallback: !cli.no_fallback,
        content_type,
        ..ParseOptions::default()
    };

    for header in &cli.headers {
        let Some((name, value)) = header.split_once('=') else {
            anyhow::bail!("headers must be NAME=VALUE, got {header:?}");
        };
        opts.headers.push((name.to_string(), value.to_string()));
    }

    let result = match &cli.file {
        Some(path) => {
            let html = std::fs::read_to_string(path)?;
            Parser::parse_html(&cli.url, &html, &opts).await
        }
        None => Parser::parse(&cli.url, &opts).await,
    };

    match result {
        Ok(article) => println!("{}", serde_json::to_string_pretty(&article)?),
        Err(err) => {
            eprintln!("{}", serde_json::to_string_pretty(&err.to_error_json())?);
            std::process::exit(1);
        }
    }

    Ok(())
}
