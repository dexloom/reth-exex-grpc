pub use client::ExExClient;

pub mod codec;

mod client;
mod helpers;
mod server;

pub mod proto {
    tonic::include_proto!("exex");
}
