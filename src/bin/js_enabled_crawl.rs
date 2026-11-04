use std::{error::Error, time::Duration};

use chromiumoxide::{Browser, BrowserConfig};
use clap::Parser;
use futures_util::StreamExt;
use regex::{Regex, RegexBuilder};
use serde::Deserialize;

#[derive(Deserialize)]
struct PageContent {
    html: String,
    links: Vec<String>,
}

#[derive(Parser, Debug)]
struct Args {
    /// HTTP(S) page to render
    #[arg(long, value_parser = parse_url)]
    url: String,
    /// Include only links with matching file extensions (case-insensitive)
    #[arg(long, value_parser = parse_extension_regex)]
    extension_regex: Option<Regex>,
}

fn parse_url(url: &str) -> Result<String, &'static str> {
    if url.starts_with("https://") || url.starts_with("http://") { Ok(url.to_owned()) } else { Err("URL must start with http:// or https://") }
}

fn parse_extension_regex(pattern: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(pattern).case_insensitive(true).build()
}

fn filter_links(links: Vec<String>, pattern: Option<&Regex>) -> Vec<String> {
    links
        .into_iter()
        .filter(|link| {
            pattern.is_none_or(|pattern| {
                let path = link.split(['?', '#']).next().unwrap_or_default();
                let name = path.rsplit('/').next().unwrap_or_default();
                name.match_indices('.').any(|(dot, _)| dot > 0 && pattern.is_match(&name[dot..]))
            })
        })
        .collect()
}

fn page_record(url: &str, html: &str, links: Vec<String>) -> serde_json::Value {
    serde_json::json!({ "requested_url": url, "html": html, "links": links })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let url = args.url.as_str();

    let (mut browser, mut handler) = Browser::launch(BrowserConfig::builder().build()?).await?;
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            event?;
        }
        Ok::<(), chromiumoxide::error::CdpError>(())
    });

    let page = browser.new_page("about:blank").await?;
    page.goto(url).await?;
    // shortcut: DOM quiet can precede late API updates; use a ready selector if a site requires one.
    let content: PageContent = tokio::time::timeout(
        Duration::from_secs(30),
        page.evaluate_function(
            r#"() => new Promise(resolve => {
                let timer;
                const observer = new MutationObserver(() => settle());
                const settle = () => {
                    if (document.readyState !== 'complete') return;
                    clearTimeout(timer);
                    timer = setTimeout(() => {
                        observer.disconnect();
                        resolve({
                            html: document.documentElement.outerHTML,
                            links: Array.from(document.querySelectorAll('a[href]'), a => a.href),
                        });
                    }, 1000);
                };
                observer.observe(document, { childList: true, subtree: true, attributes: true, characterData: true });
                window.addEventListener('load', settle, { once: true });
                settle();
            })"#,
        ),
    )
    .await
    .map_err(|_| "timed out waiting for page to finish rendering")??
    .into_value()?;
    let result = page_record(url, &content.html, filter_links(content.links, args.extension_regex.as_ref()));
    browser.close().await?;
    handler_task.await??;
    println!("{}", result);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Args, filter_links, page_record, parse_extension_regex};
    use clap::Parser;

    #[test]
    fn requires_named_http_url_and_valid_regex() {
        assert!(Args::try_parse_from(["js_enabled_crawl"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "file:///tmp/page.html"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "extra"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--extension-regex", "["]).is_err());
        let args = Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com"]).unwrap();
        assert_eq!(args.url, "https://example.com");
        assert!(args.extension_regex.is_none());
    }

    #[test]
    fn includes_only_matching_file_extensions_when_requested() {
        let links = vec![
            "https://example.com/data.CSV?download=1".into(),
            "https://example.com/archive.zip#file".into(),
            "https://example.com/data.csv.html".into(),
            "https://example.com/csv/page".into(),
        ];
        let args = Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--extension-regex", r"\.(csv|zip)$"]).unwrap();
        assert_eq!(filter_links(links.clone(), args.extension_regex.as_ref()), links[..2]);
        assert_eq!(filter_links(links.clone(), None), links);
        assert!(filter_links(vec!["https://example.com/data.xml".into()], args.extension_regex.as_ref()).is_empty());
    }

    #[test]
    fn matches_compound_and_simple_gzip_extensions() {
        let links = vec![
            "https://example.com/release.v2/data.GCT.GZ?download=1".into(),
            "https://example.com/data.txt.gz#file".into(),
            "https://example.com/data.gz".into(),
            "https://example.com/data.csv".into(),
        ];
        let compound = parse_extension_regex(r"^\.(gct\.gz|txt\.gz)$").unwrap();
        assert_eq!(filter_links(links.clone(), Some(&compound)), links[..2]);
        let gzip = parse_extension_regex(r"^\.gz$").unwrap();
        assert_eq!(filter_links(links.clone(), Some(&gzip)), links[..3]);
    }

    #[test]
    fn includes_links_even_when_empty() {
        assert_eq!(page_record("https://example.com", "<p>Text</p>", vec![]), serde_json::json!({ "requested_url": "https://example.com", "html": "<p>Text</p>", "links": [] }));
    }
}
