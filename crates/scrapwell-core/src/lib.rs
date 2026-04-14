pub mod error;
pub mod index;
pub mod model;
pub mod path;
pub mod service;
pub mod store;
mod handler;

pub use error::{Result, ScrapwellError};
pub use handler::ScrapwellHandler;
