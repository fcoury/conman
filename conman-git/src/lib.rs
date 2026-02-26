pub mod adapter;
pub mod client;
pub mod mock;
pub mod retry;

pub use adapter::{GitAdapter, NoopGitAdapter};
pub use client::GitalyClient;
pub use mock::{MockCall, MockGitalyClient};
pub use retry::{map_grpc_error, retry_grpc};
