/// Splits `text` into sentence-sized pieces on `.`, `!`, and `?`, keeping
/// the punctuation attached to the sentence it ends. Used to speak a long
/// answer one sentence at a time instead of one long `Speech::say` call:
/// `AsyncSpeech` queues each piece separately, so the first sentence starts
/// playing as soon as it's synthesized instead of waiting for the whole
/// answer to finish generating.
///
/// This is a punctuation heuristic, not a real sentence tokenizer: it will
/// split mid-sentence on things like "Sr. Smith" or "3.14". That's an
/// acceptable trade-off for TTS pacing, where an occasional early break
/// just sounds like a short pause.
pub fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        current.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            // Swallow the rest of a punctuation run ("...", "?!") so it
            // ends one sentence instead of several empty ones.
            while let Some(&next) = chars.peek() {
                if !matches!(next, '.' | '!' | '?') {
                    break;
                }
                current.push(next);
                chars.next();
            }

            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(trimmed.to_string());
            }
            current.clear();
        }
    }

    let remainder = current.trim();
    if !remainder.is_empty() {
        sentences.push(remainder.to_string());
    }

    sentences
}
