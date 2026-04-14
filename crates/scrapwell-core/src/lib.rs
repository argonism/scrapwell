pub mod error;
mod handler;
pub mod index;
pub mod model;
pub mod path;
pub mod service;
pub mod store;

pub use error::{Result, ScrapwellError};
pub use handler::ScrapwellHandler;
