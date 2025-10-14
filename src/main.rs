//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use clap::Parser;
use serde::{Deserialize, Serialize};
use zenoh::{
    Wait,
    bytes::Encoding,
    internal::{plugins::PluginsManager, runtime::RuntimeBuilder},
    key_expr::format::{kedefine, keformat},
};

mod args;
mod recorder;
mod utils;

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

    let args = args::Args::parse();
    let config = args.zenoh_config()?;
    tracing::info!("using config: {config:?}");

    // Plugin manager with REST plugin
    let mut plugins_manager = PluginsManager::static_plugins_only();
    plugins_manager.declare_static_plugin::<zenoh_plugin_rest::RestPlugin, &str>("rest", true);

    // Create a Zenoh Runtime and Session.
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

    // Create a RecorderHandler
    let mut recorder_handler =
        recorder::RecorderHandler::new(zsession.clone(), args.output_path().to_string());

    // Create a Queryable for the recorder
    let queryable_key_expr = keformat!(ke_command::formatter(), command = "*")
        .expect("should not fail to transform the key expression.");
    let queryable = zsession
        .declare_queryable(queryable_key_expr.clone())
        .await
        .map_err(|err| anyhow!("failed to subscribe to '{}': {err}", queryable_key_expr))?;

    while let Ok(query) = queryable.recv_async().await {
        let ke = if let Ok(parsed_ke) = ke_command::parse(query.key_expr()) {
            parsed_ke
        } else {
            tracing::warn!(
                "received invalid command key expression: '{}'",
                query.key_expr()
            );
            continue;
        };
        match ke.command().as_str() {
            "start" => {
                let topic = query.parameters().get("topic").unwrap_or("*");
                let domain = query.parameters().get("domain").unwrap_or("0");
                tracing::info!(
                    "received start command: topic='{:?}', domain='{:?}'",
                    topic,
                    domain
                );
                // Here we would start the recording task
                let result = if let Err(e) = recorder_handler.start(topic.to_owned(), domain) {
                    tracing::error!("failed to start recording: {e}");
                    "failure".to_owned()
                } else {
                    "success".to_owned()
                };
                let payload = match serde_json::to_string(&Start { result }) {
                    Ok(payload) => payload,
                    Err(e) => {
                        tracing::error!("failed to serialize start response: {e}");
                        continue;
                    }
                };
                if let Err(e) = query
                    .reply(query.key_expr(), payload)
                    .encoding(Encoding::TEXT_JSON)
                    .wait()
                {
                    tracing::error!("failed to send start response: {e}");
                }
            }
            "stop" => {
                tracing::info!("received stop command");
                // Here we would stop the recording task
                let (result, filename) = match recorder_handler.stop() {
                    Ok(filename) => {
                        tracing::info!("Recording stopped, saved to file: {}", filename);
                        ("success".to_owned(), filename)
                    }
                    Err(e) => {
                        tracing::error!("failed to stop recording: {e}");
                        ("failure".to_owned(), "".to_owned())
                    }
                };
                let payload = match serde_json::to_string(&Stop { result, filename }) {
                    Ok(payload) => payload,
                    Err(e) => {
                        tracing::error!("failed to serialize stop response: {e}");
                        continue;
                    }
                };
                if let Err(e) = query
                    .reply(query.key_expr(), payload)
                    .encoding(Encoding::TEXT_JSON)
                    .wait()
                {
                    tracing::error!("failed to send stop response: {e}");
                }
            }
            "status" => {
                tracing::info!("received status command");
                let status = recorder_handler.status();
                let payload = match serde_json::to_string(&Status { status }) {
                    Ok(payload) => payload,
                    Err(e) => {
                        tracing::error!("failed to serialize status response: {e}");
                        continue;
                    }
                };
                if let Err(e) = query
                    .reply(query.key_expr(), payload)
                    .encoding(Encoding::TEXT_JSON)
                    .wait()
                {
                    tracing::error!("failed to send status response: {e}");
                }
            }
            cmd => {
                tracing::warn!("unknown command received: '{cmd}'");
            }
        }
    }

    futures::future::pending::<()>().await;

    Ok(())
}
