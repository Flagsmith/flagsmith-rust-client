#[cfg(not(target_arch = "wasm32"))]
pub mod blocking_client;
pub mod client;
#[cfg(target_arch = "wasm32")]
pub mod fastly_client;
