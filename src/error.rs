use std::{error, fmt};

/// Error values for [`DRMeter`](struct.DRMeter.html) functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Not enough memory
    NoMem,
    /// Invalid channel index passed
    InvalidChannelIndex,
    /// DR Meter is finalized
    Finalized,
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::NoMem => write!(f, "NoMem"),
            Error::InvalidChannelIndex => write!(f, "Invalid Channel Index"),
            Error::Finalized => write!(f, "DR Meter instance is finalized"),
        }
    }
}
