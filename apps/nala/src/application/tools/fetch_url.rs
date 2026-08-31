use schemars::JsonSchema;
use serde::Deserialize;

use crate::application::tools::Tool;
use crate::ports::http::{HttpError, HttpFetcher};

/// How much readable text a `fetch_url` response is truncated to, so a huge
/// page doesn't blow up the model's context.
const MAX_OUTPUT_CHARS: usize = 4000;

#[derive(Deserialize, JsonSchema)]
pub struct FetchUrlArgs {
    /// The URL to fetch (http or https only).
    pub url: String,
}

pub struct FetchUrlTool<H: HttpFetcher> {
    fetcher: H,
}

impl<H: HttpFetcher> FetchUrlTool<H> {
    pub fn new(fetcher: H) -> Self {
        Self { fetcher }
    }
}

/// Strips `<script>`/`<style>` blocks, strips remaining tags, and collapses
/// whitespace, leaving plain readable text.
pub fn html_to_text(html: &str) -> String {
    let script_re = regex::Regex::new(r"(?is)<script.*?</script>").expect("script regex");
    let style_re = regex::Regex::new(r"(?is)<style.*?</style>").expect("style regex");
    let tag_re = regex::Regex::new(r"<[^>]+>").expect("tag regex");

    let without_scripts = script_re.replace_all(html, " ");
    let without_styles = style_re.replace_all(&without_scripts, " ");
    let without_tags = tag_re.replace_all(&without_styles, " ");

    without_tags
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

impl<H: HttpFetcher> Tool for FetchUrlTool<H> {
    type Args = FetchUrlArgs;
    type Output = String;
    type Error = HttpError;

    const NAME: &'static str = "fetch_url";
    const DESCRIPTION: &'static str = "Fetch a URL (http/https only) and return its readable text content, truncated to a few thousand characters. Use this to read a page found via web_search.";

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(FetchUrlArgs))
            .expect("FetchUrlArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        if !args.url.starts_with("http://") && !args.url.starts_with("https://") {
            return Err(HttpError::Request(
                "only http and https URLs are supported".to_string(),
            ));
        }

        let body = self.fetcher.get(&args.url)?;
        let text = html_to_text(&body);

        if text.chars().count() > MAX_OUTPUT_CHARS {
            let truncated: String = text.chars().take(MAX_OUTPUT_CHARS).collect();
            Ok(format!("{truncated}\n\n[Contenido truncado.]"))
        } else {
            Ok(text)
        }
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| HttpError::Request(error.to_string()))
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
