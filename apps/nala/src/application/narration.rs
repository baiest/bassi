use std::collections::HashMap;

use crate::ports::events::{Event, TurnState};
use crate::ports::narrator::Narrator;

// `Receiving`, `Planning`, `Executing`, and `Responding` deliberately don't
// narrate anything: `Executing` in particular used to say generic filler
// ("Voy a hacerlo ahora") right before `ToolStarted` says something specific
// about the same action — pure noise on top of signal. `Thinking` and
// `Verifying` are kept because they're the only states with real silent gaps
// behind them (an LLM call, a verification check).
const THINKING: &[&str] = &[
    "Déjame pensar un momento.",
    "Estoy revisando la mejor forma de hacerlo.",
    "Un segundo, estoy evaluando los pasos.",
    "Dame un momento para decidir el siguiente paso.",
];
const VERIFYING: &[&str] = &[
    "Déjame confirmar que salió bien.",
    "Voy a revisar que el resultado sea el esperado.",
];
const RETRYING: &[&str] = &[
    "Eso no funcionó, déjame intentar de otra forma.",
    "No salió como esperaba, voy a probar otra cosa.",
];
const TOOL_ERROR: &[&str] = &[
    "Eso falló, voy a intentar otra cosa.",
    "Algo salió mal ahí, dejame ajustar el enfoque.",
];
const TOOL_GENERIC: &[&str] = &[
    "Voy a usar una herramienta.",
    "Voy a hacer algo en la pantalla.",
];

/// Translates a tool call's name into a natural-language phrase. The
/// computer-use toolset is discovered dynamically at connect time (see
/// `application/tools/computer_use.rs`), so unrecognized names fall back to
/// a generic phrase instead of narrating nothing.
fn tool_phrase_bank(name: &str) -> &'static [&'static str] {
    match name {
        "screenshot" => &["Déjame ver la pantalla.", "Voy a mirar cómo va todo."],
        "execute_command" => &[
            "Voy a ejecutar un comando.",
            "Voy a correr algo en la terminal.",
        ],
        "type" => &["Voy a escribir eso.", "Ahora escribo el texto."],
        "key" => &["Presiono una tecla.", "Ahora una tecla."],
        "click" => &["Voy a hacer clic ahí.", "Hago clic en ese punto."],
        "scroll" => &["Voy a desplazar la pantalla.", "Bajo un poco la pantalla."],
        _ => TOOL_GENERIC,
    }
}

/// Speaks a canned phrase in reaction to turn events, so the user hears
/// something during the long silences between tool calls instead of dead
/// air. Two pieces of state make repeated events not sound robotic:
///
/// - a per-category rotation counter, so e.g. `Thinking` (which fires many
///   times per turn) doesn't always say the same sentence;
/// - a "last narrated category" check, so the *same* category firing twice
///   in a row (no other event in between) is narrated only once — otherwise
///   a burst of identical `Thinking` transitions would talk over itself.
#[derive(Default)]
pub struct TemplateNarrator {
    counters: HashMap<String, usize>,
    last_key: Option<String>,
}

impl TemplateNarrator {
    pub fn new() -> Self {
        Self::default()
    }

    fn pick(&mut self, key: &str, bank: &[&str]) -> Option<String> {
        if bank.is_empty() || self.last_key.as_deref() == Some(key) {
            return None;
        }

        let counter = self.counters.entry(key.to_string()).or_insert(0);
        let phrase = bank[*counter % bank.len()].to_string();
        *counter += 1;
        self.last_key = Some(key.to_string());

        Some(phrase)
    }
}

impl Narrator for TemplateNarrator {
    fn narrate(&mut self, event: &Event) -> Option<String> {
        match event {
            Event::StateChanged {
                state: TurnState::Thinking,
                ..
            } => self.pick("state:thinking", THINKING),
            Event::StateChanged {
                state: TurnState::Verifying,
                ..
            } => self.pick("state:verifying", VERIFYING),
            Event::StateChanged { .. } => None,
            Event::ToolStarted { name, .. } => {
                let key = format!("tool:{name}");
                let bank = tool_phrase_bank(name);
                self.pick(&key, bank)
            }
            Event::Retrying { .. } => self.pick("retry", RETRYING),
            // A failed LLM call is silent on its own: it's either about to
            // be retried (already narrated via `Retrying` above) or the
            // whole request is giving up, which speaks for itself once the
            // turn ends.
            Event::LlmFailed { .. } => None,
            Event::ToolCompleted { output, .. } if output.starts_with("ERROR:") => {
                self.pick("tool_error", TOOL_ERROR)
            }
            _ => None,
        }
    }
}
