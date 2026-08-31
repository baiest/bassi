use crate::fake_http_fetcher::FakeHttpFetcher;
use nala::application::tools::Tool;
use nala::application::tools::fetch_url::{FetchUrlArgs, FetchUrlTool, html_to_text};

#[test]
fn strips_script_and_style_and_tags_and_collapses_whitespace() {
    let html = r#"
        <html>
          <head><style>body { color: red; }</style></head>
          <body>
            <script>alert('hi')</script>
            <h1>Hello   World</h1>
            <p>Some   text.</p>
          </body>
        </html>
    "#;

    let text = html_to_text(html);

    assert!(!text.contains("alert"));
    assert!(!text.contains("color: red"));
    assert!(text.contains("Hello World"));
    assert!(text.contains("Some text."));
}

#[test]
fn tool_fetches_and_returns_readable_text() {
    let fetcher = FakeHttpFetcher::with_responses(vec![Ok(
        "<html><body><p>Hello there</p></body></html>".to_string(),
    )]);
    let mut tool = FetchUrlTool::new(fetcher);

    let result = tool
        .execute(FetchUrlArgs {
            url: "https://example.com".to_string(),
        })
        .expect("fetch_url should not fail");

    assert!(result.contains("Hello there"));
}

#[test]
fn tool_rejects_non_http_schemes() {
    let fetcher = FakeHttpFetcher::new();
    let mut tool = FetchUrlTool::new(fetcher);

    let result = tool.execute(FetchUrlArgs {
        url: "file:///etc/passwd".to_string(),
    });

    assert!(result.is_err());
}

#[test]
fn tool_truncates_long_output_with_a_notice() {
    let long_body = format!("<p>{}</p>", "a".repeat(10_000));
    let fetcher = FakeHttpFetcher::with_responses(vec![Ok(long_body)]);
    let mut tool = FetchUrlTool::new(fetcher);

    let result = tool
        .execute(FetchUrlArgs {
            url: "https://example.com".to_string(),
        })
        .expect("fetch_url should not fail");

    assert!(result.len() < 5000);
    assert!(result.to_lowercase().contains("truncad"));
}
