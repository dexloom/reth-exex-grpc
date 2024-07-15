pub use client::ExExClient;

pub mod codec;


mod client;
mod server;
mod helpers;

pub mod proto {
    tonic::include_proto!("exex");
}
