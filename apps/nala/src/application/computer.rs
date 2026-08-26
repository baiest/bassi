use crate::ports::computer::{Computer, ComputerError};

pub fn open_application(computer: &mut impl Computer, name: &str) -> Result<(), ComputerError> {
    computer.open_application(name)
}
