//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use zenoh::{
    Config,
    internal::{plugins::PluginsManager, runtime::RuntimeBuilder},
};

const HTTP_PORT: u16 = 8000;
const START_KEY_EXPR: &str = "@mcap_writer/start";
const STOP_KEY_EXPR: &str = "@mcap_writer/stop";

#[tokio::main]
async fn main() -> Result<()> {
    zenoh::init_log_from_env_or("z=info");

    let mut config: Config = Default::default();
    config
        .insert_json5("plugins/rest/http_port", &format!(r#""{HTTP_PORT}""#))
        .and_then(|_| config.insert_json5("plugins/rest/__required__", "true"))
        .map_err(|err| anyhow!("could not set REST HTTP port `{HTTP_PORT}`: {err}"))?;
    tracing::info!("using config: {config:?}");

    let mut plugins_manager = PluginsManager::static_plugins_only();

    plugins_manager.declare_static_plugin::<zenoh_plugin_rest::RestPlugin, &str>("rest", true);

    // create a zenoh Runtime.
    let mut runtime = RuntimeBuilder::new(config)
        .plugins_manager(plugins_manager)
        .build()
        .await
        .map_err(|err| anyhow!("failed to build Zenoh runtime: {err}"))?;

    runtime
        .start()
        .await
        .map_err(|err| anyhow!("failed to start Zenoh runtime: {err}"))?;

    let zsession = zenoh::session::init(runtime)
        .await
        .map_err(|err| anyhow!("failed to create Zenoh session: {err}"))?;
    let _queryable_start = zsession
        .declare_queryable(START_KEY_EXPR)
        .callback(move |query| {
            let key_expr = query.key_expr().clone();
            let selector = query.selector().clone();
            let value = query.payload().clone();

            tracing::info!(
                "received query on '{}': selector={:?}, value={:?}",
                key_expr,
                selector,
                value
            );
        })
        .await
        .map_err(|err| anyhow!("failed to subscribe to '{}': {err}", START_KEY_EXPR))?;

    let _queryable_stop = zsession
        .declare_queryable(STOP_KEY_EXPR)
        .callback(move |query| {
            let key_expr = query.key_expr().clone();
            let selector = query.selector().clone();
            let value = query.payload().clone();

            tracing::info!(
                "received query on '{}': selector={:?}, value={:?}",
                key_expr,
                selector,
                value
            );
        })
        .await
        .map_err(|err| anyhow!("failed to subscribe to '{}': {err}", STOP_KEY_EXPR))?;

    futures::future::pending::<()>().await;

    Ok(())
}
