//!  Implementation of the [DR Meter](https://web.archive.org/web/20180917133436/http://www.dynamicrange.de/sites/default/files/Measuring%20DR%20ENv3.pdf).

mod drmeter;
pub use self::drmeter::*;

mod block;

mod utils;

#[cfg(test)]
pub mod tests {
    pub use super::utils::tests::Signal;
}
