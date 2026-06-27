mod chapters;
mod dv_rewrite;
mod local_stream;
mod torrent_engine;

#[cfg(feature = "native")]
mod ffmpeg_locator;
#[cfg(feature = "native")]
pub mod oauth_proxy;
#[cfg(feature = "native")]
pub mod transcode;
#[cfg(feature = "native")]
pub mod companion_server;

pub mod bindings;

#[cfg(feature = "native")]
pub use torrent_engine::{start_torrent_server, stop_torrent_server};
