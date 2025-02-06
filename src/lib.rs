pub mod client;
pub mod error;
pub mod models;
pub mod parser;
pub mod service;

pub use client::DukascopyClient;
pub use error::DukascopyError;
pub use models::{CurrencyExchange, CurrencyPair};
pub use service::DukascopyFxService;
