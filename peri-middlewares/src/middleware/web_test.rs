use super::*;
use crate::middleware::web_common::{html_to_text, truncate_content, validate_url};
use crate::middleware::web_search::{
    decode_html_entities, extract_bing_results, format_search_results, resolve_bing_url,
    SearchResult,
};
use serde_json::Value;

#[test]
fn test_validate_url_rejects_ftp() {
    let err = validate_url("ftp://example.com").unwrap_err();
    assert!(err.contains("仅支持 http/https"), "实际: {err}");
}

#[test]
fn test_validate_url_rejects_localhost() {
    let err = validate_url("http://127.0.0.1/test").unwrap_err();
    assert!(err.contains("回环地址"), "实际: {err}");
}

#[test]
fn test_validate_url_rejects_private_ip() {
    let err = validate_url("http://192.168.1.1/test").unwrap_err();
    assert!(err.contains("私有地址"), "实际: {err}");
}

#[test]
fn test_validate_url_rejects_link_local() {
    let err = validate_url("http://169.254.1.1/test").unwrap_err();
    assert!(err.contains("链路本地"), "实际: {err}");
}

#[test]
fn test_validate_url_accepts_https() {
    assert!(validate_url("https://example.com/page").is_ok());
}

#[test]
fn test_validate_url_rejects_invalid_url() {
    let err = validate_url("not-a-url").unwrap_err();
    assert!(err.contains("无效的 URL"), "实际: {err}");
}

#[test]
fn test_truncate_content_no_truncation() {
    let lines: Vec<String> = (0..10).map(|i| format!("line {i}")).collect();
    let input = lines.join("\n");
    assert_eq!(truncate_content(&input, 2000), input);
}

#[test]
fn test_truncate_content_with_truncation() {
    let lines: Vec<String> = (0..3000).map(|i| format!("line {i}")).collect();
    let input = lines.join("\n");
    let result = truncate_content(&input, 2000);
    assert!(result.contains("[内容已截断，原始内容共 3000 行]"));
    assert!(result.contains("line 0"));
    assert!(result.contains("line 1999"));
    assert!(!result.contains("line 2000"));
}

#[test]
fn test_html_to_text_basic() {
    let result = html_to_text("<p>Hello</p>");
    assert!(result.contains("Hello"), "实际: {result}");
}

#[test]
fn test_tool_name_is_web_fetch() {
    assert_eq!(WebFetchTool::new().name(), "WebFetch");
}

#[test]
fn test_tool_parameters_required_url() {
    let params = WebFetchTool::new().parameters();
    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&Value::String("url".to_string())));
}

// --- WebSearchTool tests ---

#[test]
fn test_websearch_name() {
    assert_eq!(WebSearchTool::new().name(), "WebSearch");
}

#[test]
fn test_websearch_parameters_required() {
    let params = WebSearchTool::new().parameters();
    let required = params["required"].as_array().unwrap();
    assert!(required.contains(&Value::String("query".to_string())));
}

#[test]
fn test_format_search_results_empty() {
    let result = format_search_results(&[]);
    assert!(result.contains("No search results found."));
    assert!(result.contains("Web content may be inaccurate"));
}

#[test]
fn test_format_search_results_with_snippet() {
    let results = vec![
        SearchResult {
            title: "Test Page".to_string(),
            url: "https://example.com".to_string(),
            snippet: Some("A sample snippet.".to_string()),
        },
        SearchResult {
            title: "Another Page".to_string(),
            url: "https://example.org".to_string(),
            snippet: Some("Another snippet here.".to_string()),
        },
    ];
    let output = format_search_results(&results);
    assert!(output.contains("## Search Results"));
    assert!(output.contains("1. **Test Page** (https://example.com)"));
    assert!(output.contains("2. **Another Page** (https://example.org)"));
    assert!(output.contains("A sample snippet."));
}

#[test]
fn test_format_search_results_text_truncation() {
    let long_text = "x".repeat(600);
    let results = vec![SearchResult {
        title: "Long Text".to_string(),
        url: "https://example.com".to_string(),
        snippet: Some(long_text),
    }];
    let output = format_search_results(&results);
    let snippet_start = output.find("   ").unwrap() + 3;
    let snippet_end = output[snippet_start..].find("\n\n").unwrap();
    let snippet = &output[snippet_start..snippet_start + snippet_end];
    assert_eq!(snippet.chars().count(), 500);
}

#[test]
fn test_format_search_results_no_snippet() {
    let results = vec![SearchResult {
        title: "No Snippet".to_string(),
        url: "https://example.com".to_string(),
        snippet: None,
    }];
    let output = format_search_results(&results);
    assert!(output.contains("**No Snippet** (https://example.com)"));
    assert!(!output.contains("   "));
}

#[tokio::test]
async fn test_websearch_missing_query() {
    let tool = WebSearchTool::new();
    let result = tool.invoke(serde_json::json!({})).await;
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("Missing required parameter: query"),
        "实际: {err}"
    );
}

// --- Bing-specific tests ---

#[test]
fn test_resolve_bing_url_direct_external() {
    assert_eq!(
        resolve_bing_url("https://example.com/page"),
        Some("https://example.com/page".to_string())
    );
}

#[test]
fn test_resolve_bing_url_skips_relative() {
    assert_eq!(resolve_bing_url("/search?q=test"), None);
    assert_eq!(resolve_bing_url("#fragment"), None);
}

#[test]
fn test_resolve_bing_url_skips_bing_internal() {
    assert_eq!(resolve_bing_url("https://www.bing.com/search?q=test"), None);
}

#[test]
fn test_resolve_bing_url_redirect() {
    // Build a valid Bing redirect: u=a1 + base64("https://example.com")
    let target = "https://example.com";
    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        target.as_bytes(),
    );
    let redirect_url = format!("https://www.bing.com/ck/a?u=a1{b64}");
    assert_eq!(resolve_bing_url(&redirect_url), Some(target.to_string()));
}

#[test]
fn test_decode_html_entities() {
    assert_eq!(decode_html_entities("&amp;test&lt;"), "&test<");
    assert_eq!(decode_html_entities("&#39;hello&#39;"), "'hello'");
    assert_eq!(decode_html_entities("normal text"), "normal text");
}

#[test]
fn test_extract_bing_results_from_html() {
    let html = r#"<ol id="b_results"><li class="b_algo"><h2><a href="https://example.com/test">Example Title</a></h2><div class="b_caption"><p class="b_lineclamp">This is a test snippet.</p></div></li><li class="b_algo"><h2><a href="https://other.org/page">Other Title</a></h2><div class="b_caption"><p>Another snippet.</p></div></li></ol>"#;
    let results = extract_bing_results(html);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].title, "Example Title");
    assert_eq!(results[0].url, "https://example.com/test");
    assert_eq!(
        results[0].snippet.as_deref(),
        Some("This is a test snippet.")
    );
    assert_eq!(results[1].title, "Other Title");
    assert_eq!(results[1].url, "https://other.org/page");
}

#[test]
fn test_extract_bing_results_empty() {
    let html = "<html><body>No results here</body></html>";
    let results = extract_bing_results(html);
    assert!(results.is_empty());
}
