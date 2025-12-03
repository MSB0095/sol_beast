// Native implementations

pub mod http;
pub mod storage_impl;

pub use http::NativeHttpClient;
pub use storage_impl::FileStorage;
