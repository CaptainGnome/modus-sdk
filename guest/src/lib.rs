//! Guest SDK ABI 2. Authors do not copy WIT or write `wit_bindgen::generate`.
//! One feature = code preset; WIT world is always `plugin` (full guest API).

#[cfg(all(
    feature = "consumer",
    any(
        feature = "emitter",
        feature = "connector",
        feature = "provider",
        feature = "widget",
        feature = "reader",
        feature = "player",
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "emitter",
    any(
        feature = "connector",
        feature = "provider",
        feature = "widget",
        feature = "reader",
        feature = "player",
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "connector",
    any(
        feature = "provider",
        feature = "widget",
        feature = "reader",
        feature = "player",
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "provider",
    any(
        feature = "widget",
        feature = "reader",
        feature = "player",
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "widget",
    any(
        feature = "reader",
        feature = "player",
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "reader",
    any(
        feature = "player",
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "player",
    any(
        feature = "bridge",
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "bridge",
    any(
        feature = "embedder",
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "embedder",
    any(
        feature = "rates",
        feature = "alerter",
        feature = "commander",
        feature = "store",
    )
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(
    feature = "rates",
    any(feature = "alerter", feature = "commander", feature = "store")
))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(feature = "alerter", any(feature = "commander", feature = "store")))]
compile_error!("modus-sdk: exactly one role-feature");

#[cfg(all(feature = "commander", feature = "store"))]
compile_error!("modus-sdk: exactly one role-feature");

mod canon;
mod error;

pub use canon::sanitize_name_color;
pub use error::{next_backoff_ms, HostError, BACKOFF_MAX_MS, BACKOFF_START_MS};

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
pub mod bindings;

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
mod backoff;

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
pub use backoff::wait_backoff;

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
))]
pub use canon::{donation, follow, money, reward, text_fragment, text_message, viewer_count};

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
pub use bindings::exports::modus::abi::lifecycle::Guest;

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
pub use bindings::modus::abi::{assets, clock, log, self_info, settings, types, wait};

/// Store an i18n label: Core resolves `label_key` from `assets/i18n/{locale}.json`.
/// `params` — optional JSON object string, e.g. `r#"{"err":"boom"}"#`.
#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
pub fn set_label_i18n(key: &str, label_key: &str, params: Option<&str>) -> Result<(), String> {
    settings::set_label_i18n(key, label_key, params)
}

#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
pub use bindings::modus::abi::types::{SystemCode, SystemEvent};

#[cfg(any(feature = "emitter", feature = "connector"))]
pub use bindings::modus::abi::{bus_emit, chat_complete};

#[cfg(any(feature = "connector", feature = "rates"))]
pub use bindings::modus::abi::net_http;

#[cfg(any(feature = "emitter", feature = "connector"))]
pub use bindings::modus::abi::media_cache;

#[cfg(feature = "connector")]
pub use bindings::modus::abi::{auth_token, net_ws};

#[cfg(feature = "provider")]
pub use bindings::modus::abi::{catalog, media_cache, net_http, net_ws};

#[cfg(feature = "widget")]
pub use bindings::modus::abi::ui_slot;

#[cfg(feature = "reader")]
pub use bindings::modus::abi::history_read;

#[cfg(feature = "player")]
pub use bindings::modus::abi::{media_audio, media_cache};

#[cfg(feature = "bridge")]
pub use bindings::modus::abi::bridge;

#[cfg(feature = "embedder")]
pub use bindings::modus::abi::{media_embed, ui_slot};

#[cfg(feature = "rates")]
pub use bindings::modus::abi::rates_publish;

#[cfg(any(feature = "alerter", feature = "rates"))]
pub use bindings::modus::abi::rates;

#[cfg(feature = "alerter")]
pub use bindings::modus::abi::{alert_enqueue, history_read, ui_slot};

#[cfg(feature = "commander")]
pub use bindings::modus::abi::chat_act;

#[cfg(feature = "store")]
pub use bindings::modus::abi::storage_kv;

/// Export guest `lifecycle`. Write `modus_sdk::export!(MyPlugin);`
#[cfg(any(
    feature = "consumer",
    feature = "emitter",
    feature = "connector",
    feature = "provider",
    feature = "widget",
    feature = "reader",
    feature = "player",
    feature = "bridge",
    feature = "embedder",
    feature = "rates",
    feature = "alerter",
    feature = "commander",
    feature = "store",
))]
#[macro_export]
macro_rules! export {
    ($ty:ident) => {
        $crate::bindings::export!($ty with_types_in $crate::bindings);
    };
}
