use std::{
    collections::{HashSet, VecDeque},
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

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
    /// HTTP(S) pages to render
    #[arg(long, required = true, num_args = 1.., value_parser = parse_url)]
    url: Vec<String>,
    /// Number of link levels to follow (0 renders only the seed page)
    #[arg(long, default_value_t = 0)]
    depth: usize,
    /// Directory for rendered HTML (use a new directory for each crawl)
    #[arg(long)]
    temp_dir: PathBuf,
    /// Path to crawl JSON (defaults to <temp-dir>/links.json; parent directory must exist)
    #[arg(long)]
    output_path: Option<PathBuf>,
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

fn save_page(temp_dir: &Path, url: &str, html: &str, links: Vec<String>, index: usize) -> Result<serde_json::Value, Box<dyn Error>> {
    let path = url.split_once("://").and_then(|(_, rest)| rest.split_once('/')).map(|(_, path)| path).unwrap_or("");
    let name = path.split(['?', '#']).next().unwrap_or("").trim_end_matches('/').rsplit('/').next().filter(|name| !name.is_empty()).unwrap_or("index");
    let slug: String = name.strip_suffix(".html").unwrap_or(name).chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '%')).take(50).collect();
    let html_file = format!("page-{index}-{}.html", if slug.is_empty() { "index" } else { &slug });
    OpenOptions::new().write(true).truncate(true).create(true).open(temp_dir.join(&html_file))?.write_all(html.as_bytes())?;
    Ok(serde_json::json!({ "requested_url": url, "html_file": html_file, "links": links }))
}

fn queue_links(queue: &mut VecDeque<(String, usize)>, seen: &mut HashSet<String>, links: &[String], depth: usize, max_depth: usize, origin: &str) {
    if depth >= max_depth {
        return;
    }
    for link in links {
        let url = link.split('#').next().unwrap_or_default();
        if url.strip_prefix(origin).is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?'])) && seen.insert(url.to_owned()) {
            queue.push_back((url.to_owned(), depth + 1));
        }
    }
}

fn origin(url: &str) -> String {
    let (scheme, host_and_path) = url.split_once("://").unwrap();
    format!("{}://{}", scheme, host_and_path.split(['/', '?', '#']).next().unwrap())
}

fn initial_queue(urls: &[String]) -> (VecDeque<(String, usize)>, HashSet<String>) {
    let mut queue = VecDeque::new();
    let mut seen = HashSet::new();
    for url in urls {
        let seed = url.split('#').next().unwrap();
        let root = origin(url);
        if seen.insert(if seed == root { format!("{root}/") } else { seed.to_owned() }) {
            queue.push_back((url.clone(), 0));
        }
    }
    (queue, seen)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    fs::create_dir_all(&args.temp_dir)?;
    let output_path = args.output_path.unwrap_or_else(|| args.temp_dir.join("links.json"));
    let (mut queue, mut seen) = initial_queue(&args.url);
    let mut records = Vec::new();

    let (mut browser, mut handler) = Browser::launch(BrowserConfig::builder().build()?).await?;
    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            event?;
        }
        Ok::<(), chromiumoxide::error::CdpError>(())
    });

    let page = browser.new_page("about:blank").await?;
    while let Some((url, depth)) = queue.pop_front() {
        let content: Result<PageContent, Box<dyn Error>> = async {
            tokio::time::timeout(Duration::from_secs(30), page.goto(&url)).await.map_err(|_| "timed out navigating to page")??;
            // shortcut: DOM quiet can precede late API updates; use a ready selector if a site requires one.
            Ok(tokio::time::timeout(
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
            .into_value()?)
        }
        .await;
        let content = match content {
            Ok(content) => content,
            Err(error) if depth > 0 => {
                eprintln!("skipping {url}: {error}");
                continue;
            }
            Err(error) => return Err(error),
        };
        queue_links(&mut queue, &mut seen, &content.links, depth, args.depth, &origin(&url));
        records.push(save_page(&args.temp_dir, &url, &content.html, filter_links(content.links, args.extension_regex.as_ref()), records.len())?);
    }
    let mut links_file = OpenOptions::new().write(true).truncate(true).create(true).open(output_path)?;
    serde_json::to_writer_pretty(&mut links_file, &records)?;
    links_file.write_all(b"\n")?;
    browser.close().await?;
    handler_task.await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Args, filter_links, initial_queue, origin, parse_extension_regex, queue_links, save_page};
    use clap::Parser;
    use std::{
        collections::{HashSet, VecDeque},
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn requires_named_http_url_and_valid_regex() {
        assert!(Args::try_parse_from(["js_enabled_crawl"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--temp-dir", "temp-crawl"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "file:///tmp/page.html", "--temp-dir", "temp-crawl"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl", "extra"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl", "--extension-regex", "["]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com"]).is_err());
        let args = Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl"]).unwrap();
        assert_eq!(args.url, ["https://example.com"]);
        assert_eq!(args.temp_dir.to_str(), Some("temp-crawl"));
        assert!(args.output_path.is_none());
        assert_eq!(
            Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl", "--output-path", "exports/links.json"])
                .unwrap()
                .output_path
                .as_deref(),
            Some(std::path::Path::new("exports/links.json"))
        );
        assert_eq!(args.depth, 0);
        assert!(args.extension_regex.is_none());
        assert_eq!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl", "--depth", "2"]).unwrap().depth, 2);
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl", "--depth", "-1"]).is_err());
        let args = Args::try_parse_from([
            "js_enabled_crawl", "--url", "https://example.com", "https://other.org/data", "--temp-dir", "temp-crawl",
        ])
        .unwrap();
        assert_eq!(args.url, ["https://example.com", "https://other.org/data"]);
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "file:///bad", "--temp-dir", "temp-crawl"]).is_err());
        assert!(Args::try_parse_from(["js_enabled_crawl", "--url", "--temp-dir", "temp-crawl"]).is_err());
    }

    #[test]
    fn includes_only_matching_file_extensions_when_requested() {
        let links = vec![
            "https://example.com/data.CSV?download=1".into(),
            "https://example.com/archive.zip#file".into(),
            "https://example.com/data.csv.html".into(),
            "https://example.com/csv/page".into(),
        ];
        let args = Args::try_parse_from(["js_enabled_crawl", "--url", "https://example.com", "--temp-dir", "temp-crawl", "--extension-regex", r"\.(csv|zip)$"])
            .unwrap();
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
    fn saves_html_separately_and_keeps_links_even_when_empty() {
        let dir =
            std::env::temp_dir().join(format!("js-enabled-crawl-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir(&dir).unwrap();
        let record = save_page(&dir, "https://example.com", "<p>Text</p>", vec![], 0).unwrap();
        assert_eq!(record, serde_json::json!({ "requested_url": "https://example.com", "html_file": "page-0-index.html", "links": [] }));
        assert_eq!(fs::read_to_string(dir.join("page-0-index.html")).unwrap(), "<p>Text</p>");
        let record = save_page(&dir, "https://example.com/about/thisSite.html?lang=en#top", "<p>About</p>", vec![], 1).unwrap();
        assert_eq!(record["html_file"], "page-1-thisSite.html");
        assert_eq!(fs::read_to_string(dir.join("page-1-thisSite.html")).unwrap(), "<p>About</p>");
        assert!(save_page(&dir, "https://other.com/", "other", vec![], 0).is_err());
        assert_eq!(fs::read_to_string(dir.join("page-0-index.html")).unwrap(), "<p>Text</p>");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn follows_each_same_origin_page_once_up_to_depth() {
        let mut queue = VecDeque::new();
        let mut seen = HashSet::from(["https://example.com/".to_owned()]);
        let links = vec![
            "https://example.com/a#one".into(),
            "https://example.com/a#two".into(),
            "https://example.com/".into(),
            "https://example.com.evil.org/a".into(),
            "https://other.org/a".into(),
            "mailto:test@example.com".into(),
            "https://example.com/b".into(),
        ];
        queue_links(&mut queue, &mut seen, &links, 0, 2, "https://example.com");
        assert_eq!(queue, VecDeque::from([("https://example.com/a".into(), 1), ("https://example.com/b".into(), 1)]));
        queue_links(&mut queue, &mut seen, &["https://example.com/c".into()], 1, 2, "https://example.com");
        assert_eq!(queue.back(), Some(&("https://example.com/c".into(), 2)));
        queue_links(&mut queue, &mut seen, &["https://example.com/c".into()], 2, 2, "https://example.com");
        assert_eq!(queue.len(), 3);
        queue_links(&mut queue, &mut seen, &links, 0, 0, "https://example.com");
        assert_eq!(queue.len(), 3);
    }

    #[test]
    fn crawls_distinct_roots_at_depth_zero_and_follows_each_origin() {
        let (mut queue, mut seen) = initial_queue(&[
            "https://example.com".into(),
            "https://example.com/".into(),
            "https://other.org/data#top".into(),
            "https://other.org/data#bottom".into(),
        ]);
        assert_eq!(queue, VecDeque::from([("https://example.com".into(), 0), ("https://other.org/data#top".into(), 0)]));
        for (url, depth) in queue.clone() {
            let child = format!("{}/child", origin(&url));
            queue_links(&mut queue, &mut seen, &[child], depth, 1, &origin(&url));
        }
        assert_eq!(queue.pop_front(), Some(("https://example.com".into(), 0)));
        assert_eq!(queue.pop_front(), Some(("https://other.org/data#top".into(), 0)));
        assert_eq!(queue.pop_front(), Some(("https://example.com/child".into(), 1)));
        assert_eq!(queue.pop_front(), Some(("https://other.org/child".into(), 1)));
        assert!(queue.is_empty());
    }
}
