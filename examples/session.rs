//! Resolve a JMAP session via the std-blocking [`JmapClientStd`].
//!
//! Requires one of the TLS feature flags (`rustls-ring`, `rustls-aws` or
//! `native-tls`) so the client can open `https://` URLs end-to-end via
//! [`pimalaya_stream`].
//!
//! # Usage
//!
//! ```sh
//! JMAP_URL=https://api.fastmail.com/jmap/session/ \
//!   JMAP_TOKEN='Bearer <your-token>' \
//!   cargo run --example session
//! ```

use std::env;

use io_jmap::client::JmapClientStd;
use pimalaya_stream::tls::Tls;
use secrecy::SecretString;
use url::Url;

fn main() {
    env_logger::init();

    let url: Url = env::var("JMAP_URL")
        .expect("JMAP_URL env var")
        .parse()
        .expect("valid JMAP_URL");

    let http_auth = SecretString::from(env::var("JMAP_TOKEN").expect("JMAP_TOKEN env var"));

    let mut client = JmapClientStd::connect(&url, &Tls::default(), http_auth).unwrap();
    let session = client.session_get(&url).unwrap();

    println!("username: {}", session.username);
    println!("api url:  {}", session.api_url);
}
