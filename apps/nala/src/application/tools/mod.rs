pub mod execute_command;

pub trait Tool {
    type Args;
    type Output;
    type Error;

    const NAME: &'static str;
    const DESCRIPTION: &'static str;

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error>;
}
