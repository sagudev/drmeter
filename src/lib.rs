//!  Implementation of the [DR Meter](https://web.archive.org/web/20180917133436/http://www.dynamicrange.de/sites/default/files/Measuring%20DR%20ENv3.pdf).

mod block;
mod drmeter;
mod error;
mod utils;

pub use self::drmeter::*;
pub use self::error::*;

#[cfg(test)]
pub mod tests {
    pub use super::utils::tests::Signal;
}
