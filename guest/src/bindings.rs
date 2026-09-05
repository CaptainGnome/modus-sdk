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
wit_bindgen::generate!({
    path: "../../wit",
    world: "plugin",
    pub_export_macro: true,
    default_bindings_module: "modus_sdk::bindings",
});
