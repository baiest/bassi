use nala::adapters::token_counter::heuristic::HeuristicTokenCounter;
use nala::application::context_budget::{
    drop_oldest_until_fits, evict_images, truncate_long_tool_results,
};
use nala::ports::llm::Message;
use nala::ports::token_counter::TokenCounter;

fn tool_message(content: &str, images: Vec<&str>) -> Message {
    Message {
        role: "tool".to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_name: Some("screenshot".to_string()),
        images: images.into_iter().map(|image| image.to_string()).collect(),
    }
}

fn text_message(role: &str, content: &str) -> Message {
    Message {
        role: role.to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

#[test]
fn evict_images_keeps_only_the_most_recent_ones() {
    let mut messages = vec![
        text_message("system", "prompt"), // protected, index 0
        tool_message("screenshot 1", vec!["a"]),
        tool_message("screenshot 2", vec!["b"]),
        tool_message("screenshot 3", vec!["c"]),
    ];

    let dropped = evict_images(&mut messages, 1, 1);

    assert_eq!(dropped, 2, "should drop images from the two oldest results");
    assert!(messages[1].images.is_empty());
    assert!(messages[2].images.is_empty());
    assert_eq!(
        messages[3].images,
        vec!["c".to_string()],
        "the most recent image survives"
    );
}

#[test]
fn evict_images_never_touches_the_protected_prefix() {
    let mut messages = vec![tool_message("has an image", vec!["a"])];

    let dropped = evict_images(&mut messages, 1, 0);

    assert_eq!(dropped, 0);
    assert_eq!(messages[0].images, vec!["a".to_string()]);
}

#[test]
fn evict_images_is_a_no_op_when_within_the_keep_limit() {
    let mut messages = vec![tool_message("screenshot", vec!["a"])];

    let dropped = evict_images(&mut messages, 0, 5);

    assert_eq!(dropped, 0);
    assert_eq!(messages[0].images, vec!["a".to_string()]);
}

#[test]
fn truncate_long_tool_results_cuts_to_head_and_tail() {
    let long_output = "x".repeat(1000);
    let mut messages = vec![tool_message(&long_output, vec![])];

    let truncated = truncate_long_tool_results(&mut messages, 0, 10, 10);

    assert_eq!(truncated, 1);
    assert!(messages[0].content.contains("[truncated]"));
    assert!(messages[0].content.len() < long_output.len());
    assert!(messages[0].content.starts_with("xxxxxxxxxx"));
    assert!(messages[0].content.ends_with("xxxxxxxxxx"));
}

#[test]
fn truncate_long_tool_results_leaves_short_results_alone() {
    let mut messages = vec![tool_message("short", vec![])];

    let truncated = truncate_long_tool_results(&mut messages, 0, 100, 100);

    assert_eq!(truncated, 0);
    assert_eq!(messages[0].content, "short");
}

#[test]
fn truncate_long_tool_results_ignores_non_tool_messages() {
    let long_text = "x".repeat(1000);
    let mut messages = vec![text_message("assistant", &long_text)];

    let truncated = truncate_long_tool_results(&mut messages, 0, 10, 10);

    assert_eq!(truncated, 0);
    assert_eq!(messages[0].content.len(), 1000);
}

#[test]
fn drop_oldest_until_fits_removes_from_the_front_of_the_evictable_range() {
    let counter = HeuristicTokenCounter::new();
    let mut messages = vec![
        text_message("system", "prompt"),
        text_message("user", "turn 1"),
        text_message("assistant", "turn 2"),
        text_message("user", "turn 3"),
    ];

    // Budget only large enough for the protected prefix plus the very last
    // message.
    let available = counter.estimate(&messages[..1]) + counter.estimate(&messages[3..4]) + 1;

    let dropped = drop_oldest_until_fits(&mut messages, 1, available, &counter);

    assert_eq!(dropped, 2);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "prompt");
    assert_eq!(messages[1].content, "turn 3");
}

#[test]
fn drop_oldest_until_fits_never_drops_the_protected_prefix() {
    let counter = HeuristicTokenCounter::new();
    let mut messages = vec![text_message("system", &"x".repeat(500))];

    let dropped = drop_oldest_until_fits(&mut messages, 1, 0, &counter);

    assert_eq!(dropped, 0);
    assert_eq!(messages.len(), 1);
}
