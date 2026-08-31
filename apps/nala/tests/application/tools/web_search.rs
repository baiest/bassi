use crate::fake_http_fetcher::FakeHttpFetcher;
use nala::application::tools::Tool;
use nala::application::tools::web_search::{WebSearchArgs, WebSearchTool, parse_results};

const SAMPLE_HTML: &str = r#"
<div class="result results_links results_links_deep web-result">
  <div class="result__body">
    <h2 class="result__title">
      <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FRust_(programming_language)&amp;rut=abc">
        Rust (programming language) - Wikipedia
      </a>
    </h2>
    <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fen.wikipedia.org%2Fwiki%2FRust_(programming_language)">
      Rust is a multi-paradigm, general-purpose programming language.
    </a>
    <div class="result__url">en.wikipedia.org/wiki/Rust</div>
  </div>
</div>
<div class="result results_links results_links_deep web-result">
  <div class="result__body">
    <h2 class="result__title">
      <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F">
        Rust Programming Language
      </a>
    </h2>
    <a class="result__snippet" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.rust-lang.org%2F">
      A language empowering everyone to build reliable software.
    </a>
    <div class="result__url">www.rust-lang.org</div>
  </div>
</div>
"#;

#[test]
fn parses_titles_urls_and_snippets_from_ddg_html() {
    let results = parse_results(SAMPLE_HTML);

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].title, "Rust (programming language) - Wikipedia");
    assert_eq!(
        results[0].url,
        "https://en.wikipedia.org/wiki/Rust_(programming_language)"
    );
    assert!(
        results[0]
            .snippet
            .contains("multi-paradigm, general-purpose")
    );

    assert_eq!(results[1].title, "Rust Programming Language");
    assert_eq!(results[1].url, "https://www.rust-lang.org/");
}

#[test]
fn returns_no_results_for_html_with_no_matches() {
    let results = parse_results("<html><body>nothing here</body></html>");

    assert!(results.is_empty());
}

#[test]
fn tool_reports_numbered_results() {
    let fetcher = FakeHttpFetcher::with_responses(vec![Ok(SAMPLE_HTML.to_string())]);
    let mut tool = WebSearchTool::new(fetcher);

    let result = tool
        .execute(WebSearchArgs {
            query: "rust language".to_string(),
        })
        .expect("web_search should not fail");

    assert!(result.starts_with("1."));
    assert!(result.contains("Rust (programming language) - Wikipedia"));
    assert!(result.contains("2."));
}

#[test]
fn tool_reports_no_results_message_when_empty() {
    let fetcher = FakeHttpFetcher::with_responses(vec![Ok(
        "<html><body>nothing here</body></html>".to_string(),
    )]);
    let mut tool = WebSearchTool::new(fetcher);

    let result = tool
        .execute(WebSearchArgs {
            query: "asdkjaslkdjalksjd".to_string(),
        })
        .expect("web_search should not fail");

    assert!(result.to_lowercase().contains("sin resultados"));
}
