use nala::application::memory::Facts;

#[test]
fn a_new_facts_store_is_empty() {
    let facts = Facts::new();

    assert!(facts.is_empty());
    assert_eq!(facts.iter().count(), 0);
}

#[test]
fn remembering_a_key_makes_it_available() {
    let mut facts = Facts::new();

    facts.remember("nombre".to_string(), "Juan".to_string());

    let entries: Vec<(&str, &str)> = facts
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    assert_eq!(entries, vec![("nombre", "Juan")]);
}

#[test]
fn remembering_an_existing_key_overwrites_its_value() {
    // The user correcting themselves ("en realidad vivo en Bogotá") should
    // replace the old fact, not accumulate a contradictory second one.
    let mut facts = Facts::new();

    facts.remember("ciudad".to_string(), "Medellín".to_string());
    facts.remember("ciudad".to_string(), "Bogotá".to_string());

    let entries: Vec<(&str, &str)> = facts
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    assert_eq!(entries, vec![("ciudad", "Bogotá")]);
}

#[test]
fn formatting_then_parsing_round_trips() {
    let mut facts = Facts::new();
    facts.remember("nombre".to_string(), "Juan".to_string());
    facts.remember("ciudad".to_string(), "Medellín".to_string());

    let parsed = Facts::parse(&facts.to_json());

    assert_eq!(parsed, facts);
}

#[test]
fn parsing_an_empty_string_yields_an_empty_store() {
    assert_eq!(Facts::parse(""), Facts::new());
}

#[test]
fn parsing_garbage_yields_an_empty_store_instead_of_panicking() {
    // A corrupt or hand-edited memory file must never crash the assistant —
    // worst case it just starts with no memory, same as a first run.
    assert_eq!(Facts::parse("not json at all"), Facts::new());
}

#[test]
fn to_json_is_deterministic_regardless_of_insertion_order() {
    // The serialized form feeds directly into the prompt message injected
    // every turn (see agent_loop.rs) — key order must not depend on
    // insertion order, or the prompt's otherwise-stable prefix would change
    // shape across turns for no reason, defeating a backend's prompt cache.
    let mut a = Facts::new();
    a.remember("b".to_string(), "2".to_string());
    a.remember("a".to_string(), "1".to_string());

    let mut b = Facts::new();
    b.remember("a".to_string(), "1".to_string());
    b.remember("b".to_string(), "2".to_string());

    assert_eq!(a.to_json(), b.to_json());
}
