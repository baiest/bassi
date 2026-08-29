use std::collections::HashMap;

use crate::ports::events::{Event, TurnState};
use crate::ports::narrator::Narrator;

const RECEIVING: &[&str] = &["Recibí tu mensaje."];
const PLANNING: &[&str] = &["Voy a armar un plan antes de empezar."];
const THINKING: &[&str] = &[
    "Déjame pensar un momento.",
    "Estoy revisando la mejor forma de hacerlo.",
    "Un segundo, estoy evaluando los pasos.",
];
const EXECUTING: &[&str] = &["Voy a hacerlo ahora.", "Manos a la obra."];
const VERIFYING: &[&str] = &["Déjame confirmar que salió bien."];
const RESPONDING: &[&str] = &["Ya casi tengo la respuesta."];
const RETRYING: &[&str] = &["Eso no funcionó, déjame intentar de otra forma."];
const TOOL_ERROR: &[&str] = &["Eso falló, voy a intentar otra cosa."];
const TOOL_GENERIC: &[&str] = &["Voy a usar una herramienta."];

/// Translates a tool call's name into a natural-language phrase. The
/// computer-use toolset is discovered dynamically at connect time (see
/// `application/tools/computer_use.rs`), so unrecognized names fall back to
/// a generic phrase instead of narrating nothing.
fn tool_phrase_bank(name: &str) -> &'static [&'static str] {
    match name {
        "screenshot" => &["Déjame ver la pantalla."],
        "execute_command" => &["Voy a ejecutar un comando."],
        "type" => &["Voy a escribir eso."],
        "key" => &["Voy a presionar una tecla."],
        "click" => &["Voy a hacer clic ahí."],
        "scroll" => &["Voy a desplazar la pantalla."],
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
            Event::StateChanged { state } => {
                let (key, bank): (&str, &[&str]) = match state {
                    TurnState::Receiving => ("state:receiving", RECEIVING),
                    TurnState::Planning => ("state:planning", PLANNING),
                    TurnState::Thinking => ("state:thinking", THINKING),
                    TurnState::Executing => ("state:executing", EXECUTING),
                    TurnState::Verifying => ("state:verifying", VERIFYING),
                    TurnState::Responding => ("state:responding", RESPONDING),
                };
                self.pick(key, bank)
            }
            Event::ToolStarted { name, .. } => {
                let key = format!("tool:{name}");
                let bank = tool_phrase_bank(name);
                self.pick(&key, bank)
            }
            Event::Retrying { .. } => self.pick("retry", RETRYING),
            Event::ToolCompleted { output, .. } if output.starts_with("ERROR:") => {
                self.pick("tool_error", TOOL_ERROR)
            }
            _ => None,
        }
    }
}
