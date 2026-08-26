use crate::ports::computer::{Computer, ComputerError};

pub fn open_application(computer: &mut impl Computer, name: &str) -> Result<(), ComputerError> {
    computer.excecute_command(name)
}
