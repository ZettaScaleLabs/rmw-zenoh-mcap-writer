//
// Copyright (c) 2025 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: Apache-2.0
//
// Contributors:
//   ChenYing Kuo, <cy@zettascale.tech>
//
use anyhow::{Context, Result, anyhow};
use serde_json::json;
use zenoh::{Config, config::WhatAmI};
use zenoh_config::{EndPoint, ModeDependentValue};

const DEFAULT_HTTP_PORT: &str = "8000";
const DEFAULT_PATH: &str = ".";
const DEFAULT_ZENOHD: &str = "tcp/localhost:7447";

#[derive(clap::Parser, Clone, Debug)]
pub(crate) struct Args {
    /// A configuration file.
    #[arg(short, long)]
    config: Option<String>,
    /// Allows arbitrary configuration changes as column-separated KEY:VALUE pairs, where:
    ///   - KEY must be a valid config path.
    ///   - VALUE must be a valid JSON5 string that can be deserialized to the expected type for the KEY field.
    ///
    /// Example: `--cfg='transport/unicast/max_links:2'`
    #[arg(long)]
    cfg: Vec<String>,
    /// The Zenoh session mode [default: client].
    #[arg(short, long)]
    mode: Option<WhatAmI>,
    /// Locators on which Zenoh will listen for incoming connections.
    /// Repeat this option to open several listeners.
    #[arg(short, long, value_name = "ENDPOINT")]
    listen: Vec<String>,
    /// A locator Zenoh will try to connect to. By default it will try to connect to "tcp/localhost:7447".
    /// Repeat this option to connect to several routers or peers.
    #[arg(short = 'e', long, value_name = "ENDPOINT")]
    connect: Vec<String>,
    /// A timeout in milliseconds for the initial connection to a router. Default is -1, meaning no timeout.
    /// 0 means only 1 attempt of connection is made.
    #[arg(long, value_name = "ENDPOINT", default_value = "-1")]
    connect_timeout: i64,
    /// By default Zenoh replies to multicast scouting messages for being discovered by peers and clients. This option disables this feature.
    #[arg(long)]
    no_multicast_scouting: bool,
    /// Configures HTTP interface for the REST API (disabled by default). Accepted values:
    ///   - a port number
    ///   - a string with format `<local_ip>:<port_number>` (to bind the HTTP server to a specific interface)
    ///   - `none` to disable the REST API
    #[arg(long, value_name = "SOCKET")]
    rest_http_port: Option<String>,
    /// Directory where to store the recorded MCAP files.
    #[arg(short, long, value_name = "PATH", default_value = DEFAULT_PATH)]
    output_path: String,
}

impl Args {
    pub(crate) fn output_path(&self) -> &str {
        &self.output_path
    }

    pub(crate) fn zenoh_config(&self) -> Result<Config> {
        // Zenoh config
        let mut config = if let Some(fname) = self.config.as_ref() {
            Config::from_file(fname)
                .map_err(|err| anyhow!("{err}"))
                .context("could not read config file")?
        } else {
            Config::default()
        };

        // Zenoh mode
        if let Some(mode) = self.mode {
            config
                .insert_json5("mode", &json!(mode.to_str()).to_string())
                .unwrap();
        } else if self.config.is_none() {
            // default to client mode if no config file and no --mode argument
            config
                .insert_json5("mode", &json!(WhatAmI::Client.to_str()).to_string())
                .unwrap();
        }

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

        // Connect timeout
        config.connect.timeout_ms = Some(ModeDependentValue::Unique(self.connect_timeout));

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

        // JSON CFG
        for json in &self.cfg {
            if let Some((key, value)) = json.split_once(':') {
                if let Err(err) = config.insert_json5(key, value) {
                    eprintln!("`--cfg` argument: could not parse `{json}`: {err}");
                    std::process::exit(-1);
                }
            } else {
                eprintln!("`--cfg` argument: expected KEY:VALUE pair, got {json}");
                std::process::exit(-1);
            }
        }

        Ok(config)
    }
}
