use std::collections::HashSet;

use device_protocol::{CapabilityDefinition, ErrorCode, Outcome};

use crate::capability::Capability;

/// One registered capability, type-erased behind a boxed closure so
/// `CapabilityRegistry` can hold capabilities of different concrete types
/// (`ExecuteCommandTool<C>`, `OpenAppTool<C>`, ...) in one collection.
struct Entry {
    name: &'static str,
    definition: CapabilityDefinition,
    invoke: Box<dyn FnMut(&str) -> Outcome + Send>,
}

/// Every capability a device daemon can run, with an optional allowlist
/// gating both what gets announced (`definitions()`) and what can actually
/// be invoked (`invoke()`) — the daemon is the last line of defense on its
/// own machine, never trusting the allowlist to be enforced elsewhere.
pub struct CapabilityRegistry {
    entries: Vec<Entry>,
    allowed: Option<HashSet<String>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            allowed: None,
        }
    }

    pub fn with_allowlist<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            entries: Vec::new(),
            allowed: Some(names.into_iter().map(Into::into).collect()),
        }
    }

    /// Registers `capability` under its `Capability::NAME`. Registration
    /// always succeeds regardless of the allowlist — filtering happens at
    /// `definitions()`/`invoke()` time, so the allowlist can be changed
    /// without having to re-wire which capabilities exist.
    pub fn register<Cap>(&mut self, mut capability: Cap)
    where
        Cap: Capability + Send + 'static,
        Cap::Output: Into<String>,
        Cap::Error: std::fmt::Display,
    {
        let definition = Cap::definition();
        let invoke = move |arguments: &str| -> Outcome {
            match Cap::parse_arguments(arguments) {
                Ok(args) => match capability.execute(args) {
                    Ok(output) => Outcome::Ok {
                        text: output.into(),
                        mutated: Cap::MUTATING,
                    },
                    Err(error) => Outcome::Err {
                        code: ErrorCode::Failed,
                        message: error.to_string(),
                    },
                },
                Err(error) => Outcome::Err {
                    code: ErrorCode::BadArguments,
                    message: error.to_string(),
                },
            }
        };

        self.entries.push(Entry {
            name: Cap::NAME,
            definition,
            invoke: Box::new(invoke),
        });
    }

    fn is_allowed(&self, name: &str) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(name))
    }

    pub fn definitions(&self) -> Vec<CapabilityDefinition> {
        self.entries
            .iter()
            .filter(|entry| self.is_allowed(entry.name))
            .map(|entry| entry.definition.clone())
            .collect()
    }

    pub fn invoke(&mut self, name: &str, arguments: &str) -> Outcome {
        if !self.is_allowed(name) {
            return Outcome::Err {
                code: ErrorCode::Denied,
                message: format!("capability '{name}' is not allowed"),
            };
        }

        match self.entries.iter_mut().find(|entry| entry.name == name) {
            Some(entry) => (entry.invoke)(arguments),
            None => Outcome::Err {
                code: ErrorCode::NotFound,
                message: format!("unknown capability: {name}"),
            },
        }
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}
