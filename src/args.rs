//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Context, Result, anyhow};
use serde_json::json;
use zenoh::Config;
use zenoh_config::EndPoint;

const DEFAULT_HTTP_PORT: &str = "8000";
const DEFAULT_PATH: &str = ".";
const DEFAULT_ZENOHD: &str = "tcp/localhost:7447";

#[derive(clap::Parser, Clone, Debug)]
pub(crate) struct Args {
    /// The configuration file. Currently, this file must be a valid JSON5 or YAML file.
    #[arg(short, long, value_name = "PATH")]
    config: Option<String>,
    /// Locators on which this router will listen for incoming sessions. Repeat this option to open
    /// several listeners.
    #[arg(short, long, value_name = "ENDPOINT")]
    listen: Vec<String>,
    /// A peer locator this router will try to connect to.
    /// Repeat this option to connect to several peers.
    #[arg(short = 'e', long, value_name = "ENDPOINT")]
    connect: Vec<String>,
    /// By default zenohd replies to multicast scouting messages for being discovered by peers and clients. This option disables this feature.
    #[arg(long)]
    no_multicast_scouting: bool,
    /// Configures HTTP interface for the REST API (disabled by default). Accepted values:
    ///   - a port number
    ///   - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
    ///   - `none` to disable the REST API
    #[arg(long, value_name = "SOCKET")]
    rest_http_port: Option<String>,
    /// Directory where to store the recorded files.
    #[arg(short, long, value_name = "PATH", default_value = DEFAULT_PATH)]
    output_path: String,
}

impl Args {
    pub(crate) fn output_path(&self) -> &str {
        &self.output_path
    }

    pub(crate) fn zenoh_config(&self) -> Result<Config> {
        let mut config = if let Some(fname) = self.config.as_ref() {
            Config::from_file(fname)
                .map_err(|err| anyhow!("{err}"))
                .context("could not read config file")?
        } else {
            Config::default()
        };

        // REST
        // apply '--rest-http-port' to config only if explicitly set (overwriting config)
        if self.rest_http_port.is_some() {
            let value = self.rest_http_port.as_deref().unwrap_or(DEFAULT_HTTP_PORT);
            if !value.eq_ignore_ascii_case("none") {
                config
                    .insert_json5("plugins/rest/http_port", &format!(r#""{value}""#))
                    .unwrap();
                config
                    .insert_json5("plugins/rest/__required__", "true")
                    .unwrap();
            }
        }

        // Endpoints
        if !self.connect.is_empty() {
            config
                .connect
                .endpoints
                .set(
                    self.connect
                        .iter()
                        .map(|v| {
                            v.parse::<EndPoint>().map_err(|err| {
                                anyhow!("couldn't parse option --listen={v} into Locator: {err}")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .expect("setting connect/endpoints should not fail");
        } else {
            config
                .insert_json5(
                    "connect/endpoints",
                    &json!(vec![DEFAULT_ZENOHD]).to_string(),
                )
                .expect("setting connect/endpoints should not fail");
        }
        if !self.listen.is_empty() {
            config
                .listen
                .endpoints
                .set(
                    self.listen
                        .iter()
                        .map(|v| {
                            v.parse::<EndPoint>().map_err(|err| {
                                anyhow!("couldn't parse option --listen={v} into Locator: {err}")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .expect("setting listen/endpoints should not fail");
        }

        // Scouting
        let multicast_scouting = matches!(
            (
                config.scouting.multicast.enabled().is_none(),
                self.no_multicast_scouting,
            ),
            (_, true) | (true, false)
        );
        config
            .scouting
            .multicast
            .set_enabled(Some(multicast_scouting))
            .expect("setting scouting/multicast/enabled should not fail");

        Ok(config)
    }
}
