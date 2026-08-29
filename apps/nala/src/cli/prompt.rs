use std::borrow::Cow;

use reedline::{
    EditCommand, Emacs, KeyCode, KeyModifiers, Prompt, PromptEditMode, PromptHistorySearch,
    Reedline, ReedlineEvent, Signal, default_emacs_keybindings,
};

struct NalaPrompt;

impl Prompt for NalaPrompt {
    fn render_prompt_left(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_right(&self) -> Cow<'_, str> {
        Cow::Borrowed("")
    }

    fn render_prompt_indicator(&self, _edit_mode: PromptEditMode) -> Cow<'_, str> {
        Cow::Borrowed("> ")
    }

    fn render_prompt_multiline_indicator(&self) -> Cow<'_, str> {
        Cow::Borrowed(".. ")
    }

    fn render_prompt_history_search_indicator(
        &self,
        history_search: PromptHistorySearch,
    ) -> Cow<'_, str> {
        Cow::Owned(format!("(buscar: {}) ", history_search.term))
    }
}

/// Reads a message from the terminal, letting the user write or paste
/// several lines and freely edit them (arrows, Backspace/Delete across
/// lines) before sending. Plain Enter inserts a newline; Ctrl+Enter sends —
/// classic Windows consoles deliver a pasted newline as a plain Enter
/// keypress, so "Enter submits" would split a pasted message apart.
///
/// Not covered by the automated test suite: this is a thin composition of
/// `reedline`'s own (independently tested) editor, with no branching logic
/// of our own left to exercise.
pub struct MultilineReader {
    editor: Reedline,
}

impl MultilineReader {
    pub fn new() -> Self {
        let mut keybindings = default_emacs_keybindings();
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Enter,
            ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
        );
        keybindings.add_binding(KeyModifiers::CONTROL, KeyCode::Enter, ReedlineEvent::Enter);

        let editor = Reedline::create().with_edit_mode(Box::new(Emacs::new(keybindings)));

        Self { editor }
    }

    /// Returns `None` on Ctrl+C / Ctrl+D (the user wants to exit).
    pub fn read(&mut self) -> std::io::Result<Option<String>> {
        match self.editor.read_line(&NalaPrompt) {
            Ok(Signal::Success(text)) => Ok(Some(text)),
            Ok(Signal::CtrlC) | Ok(Signal::CtrlD) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl Default for MultilineReader {
    fn default() -> Self {
        Self::new()
    }
}
