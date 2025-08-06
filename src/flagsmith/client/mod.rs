#[cfg(not(feature = "non_blocking"))]
pub mod blocking_client;
pub mod client;
#[cfg(feature = "non_blocking")]
pub mod fastly_client;
