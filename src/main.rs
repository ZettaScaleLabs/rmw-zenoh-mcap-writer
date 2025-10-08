//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zenoh::{
    Config, Wait,
    bytes::Encoding,
    internal::{plugins::PluginsManager, runtime::RuntimeBuilder},
    key_expr::format::{kedefine, keformat},
};

mod recorder;

// TODO: Make it configurable
const HTTP_PORT: u16 = 8000;
// TODO: Make it configurable
const DEFAULT_PATH: &str = ".";
// TODO: Make it configurable
const DEFAULT_ZENOHD: &str = "tcp/localhost:7447";

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

    // TODO: Able to accept customized config file
    let mut config: Config = Default::default();
    config
        .insert_json5("plugins/rest/http_port", &format!(r#""{HTTP_PORT}""#))
        .and_then(|_| config.insert_json5("plugins/rest/__required__", "true"))
        .map_err(|err| anyhow!("could not set REST HTTP port `{HTTP_PORT}`: {err}"))?;
    config
        .insert_json5(
            "connect/endpoints",
            &json!(vec![DEFAULT_ZENOHD]).to_string(),
        )
        .unwrap();
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
    let zsession = zenoh::session::init(runtime.into())
        .await
        .map_err(|err| anyhow!("failed to create Zenoh session: {err}"))?;

    // Create a RecorderHandler
    let mut recorder_handler =
        recorder::RecorderHandler::new(zsession.clone(), DEFAULT_PATH.to_string());

    // Create a Queryable for the recorder
    let queryable_key_expr = keformat!(ke_command::formatter(), command = "*").unwrap();
    let queryable = zsession
        .declare_queryable(queryable_key_expr.clone())
        .await
        .map_err(|err| anyhow!("failed to subscribe to '{}': {err}", queryable_key_expr))?;

    while let Ok(query) = queryable.recv_async().await {
        let ke = ke_command::parse(query.key_expr()).unwrap();
        match ke.command().as_str() {
            "start" => {
                // TODO: Need to parse a list of topics
                let topic = query.parameters().get("topic").unwrap_or("*");
                let domain = query.parameters().get("domain").unwrap_or("0");
                tracing::info!(
                    "received start command: topic='{:?}', domain='{:?}'",
                    topic,
                    domain
                );
                // Here we would start the recording task
                let result = if let Err(e) =
                    recorder_handler.start(topic.to_owned(), domain.parse().unwrap())
                {
                    tracing::error!("failed to start recording: {e}");
                    "failure".to_owned()
                } else {
                    "success".to_owned()
                };
                let start = Start { result };
                query
                    .reply(query.key_expr(), serde_json::to_string(&start).unwrap())
                    .encoding(Encoding::TEXT_JSON)
                    .wait()
                    .unwrap();
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
                let stop = Stop { result, filename };
                query
                    .reply(query.key_expr(), serde_json::to_string(&stop).unwrap())
                    .encoding(Encoding::TEXT_JSON)
                    .wait()
                    .unwrap();
            }
            "status" => {
                tracing::info!("received status command");
                let status = recorder_handler.status();
                let status = Status { status };
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
    }

    futures::future::pending::<()>().await;

    Ok(())
}
