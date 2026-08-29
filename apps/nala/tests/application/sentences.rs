use nala::application::sentences::split_into_sentences;

#[test]
fn splits_on_period_exclamation_and_question_mark() {
    let text = "Hola. Como estas? Que bueno verte!";

    let sentences = split_into_sentences(text);

    assert_eq!(sentences, vec!["Hola.", "Como estas?", "Que bueno verte!"]);
}

#[test]
fn returns_the_whole_text_when_there_is_no_terminal_punctuation() {
    let text = "Un texto sin punto final";

    let sentences = split_into_sentences(text);

    assert_eq!(sentences, vec!["Un texto sin punto final"]);
}

#[test]
fn collapses_repeated_punctuation_like_an_ellipsis() {
    let text = "Esperá... que raro?!";

    let sentences = split_into_sentences(text);

    assert_eq!(sentences, vec!["Esperá...", "que raro?!"]);
}

#[test]
fn trims_whitespace_around_each_sentence() {
    let text = "  Hola.   Adios.  ";

    let sentences = split_into_sentences(text);

    assert_eq!(sentences, vec!["Hola.", "Adios."]);
}

#[test]
fn returns_nothing_for_blank_input() {
    let sentences = split_into_sentences("   ");

    assert!(sentences.is_empty());
}
