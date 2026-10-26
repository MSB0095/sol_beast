//! SOL BEAST DEX layer: a complete venue catalogue, verified account decoders,
//! pure AMM math and a route graph. The catalogue intentionally lists every
//! venue we know about, including prop AMMs/CLOBs that can only be quoted via
//! Jupiter; coverage is explicit so Radar never pretends to model them locally.

pub mod address;
pub mod catalog;
pub mod decode;
pub mod error;
pub mod jupiter;
pub mod math;
pub mod model;
pub mod router;

pub use address::Address;
pub use catalog::{Coverage, DecoderKind, MathModel, Venue, VenueKind, VENUES};
pub use error::{DexError, Result};
pub use model::{PoolModel, PoolSnapshot, Quote, RouteLeg, RouteQuote};
