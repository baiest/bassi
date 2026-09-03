use nala::adapters::memory::in_memory::InMemoryMemoryStore;
use nala::application::tools::Tool;
use nala::application::tools::remember::RememberTool;

#[test]
fn executing_stores_the_fact_in_the_underlying_store() {
    let mut tool = RememberTool::new(Box::new(InMemoryMemoryStore::new()));

    let args = RememberTool::parse_arguments(r#"{"key":"nombre","value":"Juan"}"#).unwrap();
    tool.execute(args).unwrap();

    assert_eq!(
        tool.facts(),
        vec![("nombre".to_string(), "Juan".to_string())]
    );
}

#[test]
fn execute_returns_a_confirmation_naming_the_fact() {
    let mut tool = RememberTool::new(Box::new(InMemoryMemoryStore::new()));

    let args = RememberTool::parse_arguments(r#"{"key":"nombre","value":"Juan"}"#).unwrap();
    let text = tool.execute(args).unwrap();

    assert!(text.contains("nombre"));
    assert!(text.contains("Juan"));
}

// `MUTATING` is exercised end-to-end by
// `dispatcher::routes_a_remember_call_to_the_remember_tool_and_marks_it_mutated`
// rather than asserted here directly — clippy flags `assert!` on a
// compile-time constant as pointless (`assertions_on_constants`).

#[test]
fn rejects_arguments_missing_a_field() {
    let result = RememberTool::parse_arguments(r#"{"key":"nombre"}"#);

    assert!(result.is_err());
}

#[test]
fn definition_advertises_key_and_value_parameters() {
    let definition = RememberTool::definition();

    assert_eq!(definition.name, "remember");
    let schema = definition.parameters.to_string();
    assert!(schema.contains("key"));
    assert!(schema.contains("value"));
}
