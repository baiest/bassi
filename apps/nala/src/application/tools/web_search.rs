use schemars::JsonSchema;
use serde::Deserialize;

use crate::application::tools::Tool;
use crate::ports::http::{HttpError, HttpFetcher};

#[derive(Deserialize, JsonSchema)]
pub struct WebSearchArgs {
    /// The search query.
    pub query: String,
}

pub struct WebSearchTool<H: HttpFetcher> {
    fetcher: H,
}

impl<H: HttpFetcher> WebSearchTool<H> {
    pub fn new(fetcher: H) -> Self {
        Self { fetcher }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Pulls the top results out of DuckDuckGo's HTML search endpoint. Hand-
/// rolled rather than a full HTML parser: DDG's markup is regular enough
/// that matching `result__a` / `result__snippet` blocks via regex is
/// reliable and avoids pulling in a heavier dependency.
pub fn parse_results(html: &str) -> Vec<SearchResult> {
    let link_re = regex::Regex::new(r#"(?s)class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
        .expect("result__a regex should compile");
    let snippet_re = regex::Regex::new(r#"(?s)class="result__snippet"[^>]*>(.*?)</a>"#)
        .expect("result__snippet regex should compile");
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("tag-strip regex should compile");

    let links: Vec<(String, String)> = link_re
        .captures_iter(html)
        .map(|capture| {
            let href = capture[1].to_string();
            let title = clean_text(&tag_re.replace_all(&capture[2], ""));
            (href, title)
        })
        .collect();

    let snippets: Vec<String> = snippet_re
        .captures_iter(html)
        .map(|capture| clean_text(&tag_re.replace_all(&capture[1], "")))
        .collect();

    links
        .into_iter()
        .zip(snippets)
        .map(|((href, title), snippet)| SearchResult {
            title,
            url: decode_ddg_redirect(&href),
            snippet,
        })
        .collect()
}

fn clean_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// DDG's HTML endpoint wraps result hrefs in a redirect like
/// `//duckduckgo.com/l/?uddg=<url-encoded-target>&rut=...`. Extract and
/// URL-decode the `uddg` param to get the real target URL; if the href
/// isn't wrapped that way (unexpected shape), return it unchanged.
fn decode_ddg_redirect(href: &str) -> String {
    let Some(query_start) = href.find('?') else {
        return href.to_string();
    };
    let query = &href[query_start + 1..];

    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("uddg=") {
            return url_decode(value);
        }
    }

    href.to_string()
}

fn url_decode(value: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = value.bytes().peekable();

    while let Some(byte) = chars.next() {
        match byte {
            b'%' => {
                let hi = chars.next();
                let lo = chars.next();
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    let hex = [hi, lo];
                    if let Ok(hex_str) = std::str::from_utf8(&hex)
                        && let Ok(value) = u8::from_str_radix(hex_str, 16)
                    {
                        bytes.push(value);
                        continue;
                    }
                }
            }
            b'+' => bytes.push(b' '),
            other => bytes.push(other),
        }
    }

    String::from_utf8_lossy(&bytes).to_string()
}

impl<H: HttpFetcher> Tool for WebSearchTool<H> {
    type Args = WebSearchArgs;
    type Output = String;
    type Error = HttpError;

    const NAME: &'static str = "web_search";
    const DESCRIPTION: &'static str = "Search the web for a query and return the top results (title, URL, snippet). Use this for factual or current-events questions instead of guessing.";

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(WebSearchArgs))
            .expect("WebSearchArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let url = format!(
            "https://html.duckduckgo.com/html/?q={}",
            urlencode(&args.query)
        );
        let body = self.fetcher.get(&url)?;
        let results = parse_results(&body);

        if results.is_empty() {
            return Ok("Sin resultados para esa búsqueda.".to_string());
        }

        let text = results
            .iter()
            .take(5)
            .enumerate()
            .map(|(index, result)| {
                format!(
                    "{}. {} — {}\n   {}",
                    index + 1,
                    result.title,
                    result.snippet,
                    result.url
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(text)
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| HttpError::Request(error.to_string()))
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}

fn urlencode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
