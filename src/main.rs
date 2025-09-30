//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use zenoh::internal::{plugins::PluginsManager, runtime::RuntimeBuilder};

#[tokio::main]
async fn main() -> Result<()> {
    zenoh::init_log_from_env_or("z=info");

    let config = Default::default();
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

    futures::future::pending::<()>().await;

    Ok(())
}
