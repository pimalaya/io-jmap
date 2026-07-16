//! Exercise the JMAP PushSubscription lifecycle (RFC 8620 §7.2) via the
//! std-blocking [`JmapClientStd`]: list, create, verify, destroy.
//!
//! Requires one of the TLS feature flags (`rustls-ring`, `rustls-aws` or
//! `native-tls`) so the client can open `https://` URLs end-to-end via
//! [`pimalaya_stream`].
//!
//! # Usage
//!
//! List existing push subscriptions:
//!
//! ```sh
//! JMAP_URL=https://api.fastmail.com/jmap/session/ \
//!   JMAP_TOKEN='Bearer <your-token>' \
//!   cargo run --example push_subscription
//! ```
//!
//! Create a subscription (the server then POSTs a PushVerification object to
//! the given URL; optionally restrict pushed types with a comma-separated
//! JMAP_PUSH_TYPES, and set your own device ID with JMAP_PUSH_DEVICE_ID):
//!
//! ```sh
//! JMAP_PUSH_URL='https://push.example.com/?device=X8980fc' \
//!   JMAP_URL=… JMAP_TOKEN=… cargo run --example push_subscription
//! ```
//!
//! Verify it with the code received on the push URL:
//!
//! ```sh
//! JMAP_PUSH_ID=P1 JMAP_PUSH_CODE=b210ef734fe5f439c1ca386421359f7b \
//!   JMAP_URL=… JMAP_TOKEN=… cargo run --example push_subscription
//! ```
//!
//! Destroy it:
//!
//! ```sh
//! JMAP_PUSH_DESTROY=P1 \
//!   JMAP_URL=… JMAP_TOKEN=… cargo run --example push_subscription
//! ```

use std::env;

use io_jmap::{
    client::JmapClientStd,
    rfc8620::push_subscription::{
        get::JmapPushSubscriptionGetOptions,
        set::{
            JmapPushSubscriptionCreate, JmapPushSubscriptionSetArgs, JmapPushSubscriptionUpdate,
        },
    },
};
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
    client.session_get(&url).unwrap();

    if let Ok(push_url) = env::var("JMAP_PUSH_URL") {
        let device_client_id =
            env::var("JMAP_PUSH_DEVICE_ID").unwrap_or_else(|_| "io-jmap-example".into());
        let types = env::var("JMAP_PUSH_TYPES")
            .ok()
            .map(|types| types.split(',').map(Into::into).collect());

        let mut args = JmapPushSubscriptionSetArgs::default();
        args.create(
            "c1",
            JmapPushSubscriptionCreate {
                device_client_id,
                url: push_url,
                types,
                ..Default::default()
            },
        );

        let out = client.push_subscription_set(args).unwrap();

        for sub in out.created.values() {
            println!("created subscription {}, awaiting verification", sub.id);
        }
        for (client_id, err) in &out.not_created {
            println!("not created {client_id}: {}", err.r#type);
        }
    }

    if let (Ok(id), Ok(code)) = (env::var("JMAP_PUSH_ID"), env::var("JMAP_PUSH_CODE")) {
        let mut args = JmapPushSubscriptionSetArgs::default();
        args.update(
            &id,
            JmapPushSubscriptionUpdate {
                verification_code: Some(code),
                ..Default::default()
            },
        );

        let out = client.push_subscription_set(args).unwrap();

        if out.updated.contains_key(&id) {
            println!("verified subscription {id}");
        }
        for (id, err) in &out.not_updated {
            println!("not updated {id}: {}", err.r#type);
        }
    }

    if let Ok(id) = env::var("JMAP_PUSH_DESTROY") {
        let mut args = JmapPushSubscriptionSetArgs::default();
        args.destroy(&id);

        let out = client.push_subscription_set(args).unwrap();

        for id in &out.destroyed {
            println!("destroyed subscription {id}");
        }
        for (id, err) in &out.not_destroyed {
            println!("not destroyed {id}: {}", err.r#type);
        }
    }

    let out = client
        .push_subscription_get(JmapPushSubscriptionGetOptions::default())
        .unwrap();

    println!("{} push subscription(s):", out.subscriptions.len());

    for sub in &out.subscriptions {
        println!(
            "- id: {}, device: {}, verified: {}, expires: {}, types: {}",
            sub.id,
            sub.device_client_id.as_deref().unwrap_or("<none>"),
            sub.verification_code.is_some(),
            sub.expires.as_deref().unwrap_or("<none>"),
            sub.types
                .as_ref()
                .map(|types| types.join(","))
                .unwrap_or_else(|| "<all>".into()),
        );
    }
}
