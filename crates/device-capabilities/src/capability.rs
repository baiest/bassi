use device_protocol::CapabilityDefinition;

/// Same shape as `nala`'s `Tool` trait, kept as a separate trait rather than
/// shared: `nala` depends on this crate, so this crate can't implement a
/// trait defined in `nala` without a dependency cycle.
pub trait Capability {
    type Args;
    type Output;
    type Error;

    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    /// See `nala`'s `Tool::MUTATING` — same meaning, used the same way by
    /// the verification gate on the far side of a remote invocation.
    const MUTATING: bool = false;

    fn parameters() -> serde_json::Value;

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error>;

    fn parse_arguments(arguments: &str) -> Result<Self::Args, Self::Error>;

    fn definition() -> CapabilityDefinition {
        CapabilityDefinition {
            name: Self::NAME.to_string(),
            description: Self::DESCRIPTION.to_string(),
            parameters: Self::parameters(),
        }
    }

    fn context(&mut self) -> Result<String, Self::Error>;
}
