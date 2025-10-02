//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use zenoh::{
    Config, Wait,
    bytes::Encoding,
    internal::{plugins::PluginsManager, runtime::RuntimeBuilder},
    key_expr::format::{kedefine, keformat},
};

//mod recorder;

const HTTP_PORT: u16 = 8000;
kedefine!(
    pub(crate) ke_command: "@mcap/writer/${command:*}",
);

#[tokio::main]
async fn main() -> Result<()> {
    zenoh::init_log_from_env_or("info");

    let mut config: Config = Default::default();
    config
        .insert_json5("plugins/rest/http_port", &format!(r#""{HTTP_PORT}""#))
        .and_then(|_| config.insert_json5("plugins/rest/__required__", "true"))
        .map_err(|err| anyhow!("could not set REST HTTP port `{HTTP_PORT}`: {err}"))?;
    tracing::info!("using config: {config:?}");

    // Plugin manager with REST plugin
    let mut plugins_manager = PluginsManager::static_plugins_only();
    plugins_manager.declare_static_plugin::<zenoh_plugin_rest::RestPlugin, &str>("rest", true);

    // Create a Zenoh Runtime.
    let mut runtime = RuntimeBuilder::new(config)
        .plugins_manager(plugins_manager)
        .build()
        .await
        .map_err(|err| anyhow!("failed to build Zenoh runtime: {err}"))?;
    runtime
        .start()
        .await
        .map_err(|err| anyhow!("failed to start Zenoh runtime: {err}"))?;

    // Create a Queryable for the recorder
    let zsession = zenoh::session::init(runtime)
        .await
        .map_err(|err| anyhow!("failed to create Zenoh session: {err}"))?;
    let queryable_key_expr = keformat!(ke_command::formatter(), command = "*").unwrap();
    let _queryable = zsession
        .declare_queryable(queryable_key_expr.clone())
        .callback(move |query| {
            let ke = ke_command::parse(query.key_expr()).unwrap();
            match ke.command().as_str() {
                "start" => {
                    let topic = query.parameters().get("topic");
                    let domain = query.parameters().get("domain");
                    tracing::info!(
                        "received start command: topic='{:?}', domain='{:?}'",
                        topic,
                        domain
                    );
                    // Here we would start the recording task
                    //let _record_task = recorder::RecordTask::new(topic, domain);
                    query
                        .reply(query.key_expr(), "success")
                        .encoding(Encoding::TEXT_PLAIN)
                        .wait()
                        .unwrap();
                }
                "stop" => {
                    tracing::info!("received stop command");
                    // Here we would stop the recording task
                    query
                        .reply(query.key_expr(), "success")
                        .encoding(Encoding::TEXT_PLAIN)
                        .wait()
                        .unwrap();
                }
                "status" => {
                    tracing::info!("received status command");
                    // Here we would return the status of the recorder
                    query
                        .reply(query.key_expr(), "running")
                        .encoding(Encoding::TEXT_PLAIN)
                        .wait()
                        .unwrap();
                }
                cmd => {
                    tracing::warn!("unknown command received: '{cmd}'");
                }
            }
        })
        .await
        .map_err(|err| anyhow!("failed to subscribe to '{}': {err}", queryable_key_expr))?;

    futures::future::pending::<()>().await;

    Ok(())
}
