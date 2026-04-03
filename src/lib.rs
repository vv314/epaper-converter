pub mod cli;
pub(crate) mod pipeline;
pub(crate) mod quantize;

pub use cli::run;

#[cfg(test)]
mod tests;
