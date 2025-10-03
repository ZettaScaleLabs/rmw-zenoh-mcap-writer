//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use chrono::Local;
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Start {
    pub(crate) result: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Stop {
    pub(crate) result: String,
    pub(crate) filename: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Status {
    pub(crate) status: String,
}

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
                    // TODO: Get the exact result: success / failure
                    let start = Start {
                        result: "success".to_string(),
                    };
                    query
                        .reply(query.key_expr(), serde_json::to_string(&start).unwrap())
                        .encoding(Encoding::TEXT_JSON)
                        .wait()
                        .unwrap();
                }
                "stop" => {
                    tracing::info!("received stop command");
                    // Here we would stop the recording task
                    // TODO: Get the exact result: success / failure
                    // TODO: filename should be based on actual recording
                    let now = Local::now();
                    let stop = Stop {
                        result: "success".to_string(),
                        filename: now.format("rosbag2_%Y_%m_%d-%H_%M_%S").to_string(),
                    };
                    query
                        .reply(query.key_expr(), serde_json::to_string(&stop).unwrap())
                        .encoding(Encoding::TEXT_JSON)
                        .wait()
                        .unwrap();
                }
                "status" => {
                    tracing::info!("received status command");
                    // Here we would return the status of the recorder
                    // TODO: Get the real status (recording / stopped)
                    let status = Status {
                        status: "recording".to_string(),
                    };
                    query
                        .reply(query.key_expr(), serde_json::to_string(&status).unwrap())
                        .encoding(Encoding::TEXT_JSON)
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
