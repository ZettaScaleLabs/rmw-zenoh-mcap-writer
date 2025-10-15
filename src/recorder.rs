//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use std::{collections::BTreeMap, fs, io::BufWriter};

use anyhow::{Result, anyhow};
use chrono::{Local, Utc};
use mcap::{Writer, records::MessageHeader, write::Metadata};
use tokio::sync::oneshot;
use zenoh::{
    Session,
    key_expr::{
        OwnedKeyExpr,
        format::{kedefine, keformat},
        keyexpr,
    },
    sample::SampleKind,
};

use crate::utils;

kedefine!(
    pub(crate) ke_rostopic: "${domain:*}/${topic:*}/${rostype:*}/${hash:*}",
    // TODO: Should we consider mangled_enclave and mangled_namespace
    pub(crate) ke_graphcache: "@ros2_lv/${domain:*}/${zid:*}/${node:*}/${entity:*}/MP/%/%/${node_name:*}/${topic:*}/${rostype:*}/${hash:*}/${qos:*}",
);

pub struct RecorderHandler {
    session: Session,
    path: String,
    task: Option<RecordTask>,
}

impl RecorderHandler {
    pub fn new(session: Session, path: String) -> Self {
        Self {
            session,
            path,
            task: None,
        }
    }

    pub fn start(&mut self, topic: String, domain: &str, ros_distro: String) -> Result<String> {
        if self.task.is_some() {
            return Err(anyhow!("Recording task is already running"));
        }
        let domain = domain
            .parse::<u32>()
            .map_err(|e| anyhow!("Invalid domain '{}': {}", domain, e))?;
        let topics: Vec<OwnedKeyExpr> = topic
            .split(',')
            .map(|s| {
                // Remove the space between string
                let trimmed = s.trim();
                // Remove leading "/"
                trimmed.strip_prefix('/').unwrap_or(trimmed)
            })
            // Remove the empty string
            .filter(|s| !s.is_empty())
            // Transform into key_expr
            .map(|s| OwnedKeyExpr::new(s).unwrap())
            .collect();
        let record_task = RecordTask::new(
            self.session.clone(),
            self.path.clone(),
            topics,
            domain,
            ros_distro,
        );
        self.task = Some(record_task);
        Ok("Recording started".to_string())
    }

    pub fn status(&mut self) -> String {
        if let Some(ref task) = self.task
            && !task.is_finished()
        {
            return "recording".to_string();
        }
        self.task.take(); // Drop the task
        "stopped".to_string()
    }

    pub fn stop(&mut self) -> Result<String> {
        if let Some(task) = self.task.take() {
            let filename = task.filename.clone();
            tokio::spawn(async move {
                task.stop().await;
            });
            Ok(filename)
        } else {
            Err(anyhow!("No recording task is running"))
        }
    }
}

struct RecordTask {
    handle: tokio::task::JoinHandle<()>,
    stop_tx: oneshot::Sender<()>,
    filename: String,
}

impl RecordTask {
    fn new(
        session: Session,
        path: String,
        topics: Vec<OwnedKeyExpr>,
        domain: u32,
        ros_distro: String,
    ) -> Self {
        let (stop_tx, stop_rx) = oneshot::channel();
        let filename = format!("rosbag2_{}.mcap", Local::now().format("%Y_%m_%d_%H_%M_%S"));
        let filename_clone = filename.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = RecordTask::write_mcap(
                session,
                path,
                topics,
                domain,
                stop_rx,
                filename_clone,
                ros_distro,
            )
            .await
            {
                tracing::error!("Fail while running the record task: {e}");
            }
        });

        Self {
            handle,
            stop_tx,
            filename,
        }
    }

    async fn write_mcap(
        session: Session,
        path: String,
        topics: Vec<OwnedKeyExpr>,
        domain: u32,
        mut stop_rx: oneshot::Receiver<()>,
        filename: String,
        ros_distro: String,
    ) -> Result<()> {
        let fullpath = format!("{}/{}", path, filename);
        tracing::debug!(
            "Started recording topic '{:?}' on domain {}",
            topics,
            domain
        );
        // create a hashmap to store schema (String => u16)
        let mut schemas_map: BTreeMap<String, u16> = BTreeMap::new();
        let mut channels_map: BTreeMap<String, u16> = BTreeMap::new();
        let options = mcap::WriteOptions::default()
            .profile("ros2")
            .calculate_chunk_crcs(false)
            .compression(None);
        let mut out = Writer::with_options(BufWriter::new(fs::File::create(fullpath)?), options)?;
        // TODO: Write the metadata
        let metadata = utils::BagMetadata::new(&filename, ros_distro)?;
        out.write_metadata(&Metadata {
            name: "rosbag2".to_string(),
            metadata: BTreeMap::from([(
                "serialized_metadata".to_string(),
                metadata.to_yaml_string()?,
            )]),
        })?;

        // Subscribe to the topic
        let key_expr = keformat!(
            ke_rostopic::formatter(),
            domain = domain,
            topic = "*",
            rostype = "*",
            hash = "*",
        )
        .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
        tracing::debug!("Subscribing to key expression: {}", key_expr);
        let subscriber = session
            .declare_subscriber(&key_expr)
            .await
            .map_err(|e| anyhow!("Unable to declare the subscriber: {e}"))?;

        // Subscribe to the liveliness
        let key_expr = keformat!(
            ke_graphcache::formatter(),
            domain = domain,
            zid = "*",
            node = "*",
            entity = "*",
            node_name = "*",
            topic = "*",
            rostype = "*",
            hash = "*",
            qos = "*",
        )
        .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
        tracing::debug!("Subscribing to liveliness key expression: {}", key_expr);
        let liveliness_subscriber = session
            .liveliness()
            .declare_subscriber(&key_expr)
            .history(true)
            .await
            .map_err(|e| anyhow!("Unable to declare the liveliness_subscriber: {e}"))?;
        loop {
            tokio::select! {
                sample = subscriber.recv_async() => {
                    if let Ok(sample) = sample {
                        tracing::trace!(
                            "Received sample on topic '{}': {:?}",
                            sample.key_expr(),
                            sample.payload()
                        );
                        if let Ok(ke) = ke_rostopic::parse(sample.key_expr()) {
                            tracing::debug!("topic: {}, rostype: {}, hash: {}", ke.topic(), ke.rostype(), ke.hash());
                            // Filter the topic which is not recorded
                            let any_match = topics.iter().any(|t| t.includes(ke.topic()));
                            if !any_match {
                                tracing::debug!("topic {} is not in the recorded list, skipping...", ke.topic());
                                continue;
                            }
                            let topic = "/".to_string() + ke.topic();  // The topic requires the leading '/'
                            if let Some(channel_id) = channels_map.get(&topic) {
                                tracing::debug!("Found existing channel_id: {}", channel_id);
                                let current_time = Utc::now().timestamp_nanos_opt().ok_or(anyhow!("Unable to get the current time"))? as u64;
                                out.write_to_known_channel(
                                    &MessageHeader {
                                        channel_id: *channel_id,
                                        sequence: 0,  // It should be 0 if we don't use the sequence
                                        log_time: current_time,  // Receive timestamp
                                        publish_time: current_time,  // TODO: Parse the Zenoh attachment
                                    },
                                    sample.payload().to_bytes().as_ref(),
                                )?;
                            } else {
                                tracing::debug!("Skip the messages because there is no existing channel_id for topic: {}", topic);
                            }

                        }
                    } else {
                        tracing::error!("Error receiving sample");
                        break;
                    }
                },
                sample = liveliness_subscriber.recv_async() => {
                    if let Ok(sample) = sample {
                        // Ignore non-Put samples in liveliness
                        if sample.kind() != SampleKind::Put {
                            continue;
                        }
                        tracing::trace!(
                            "Received liveliness sample on topic '{}': {:?}",
                            sample.key_expr(),
                            sample.payload()
                        );
                        if let Ok(ke) = ke_graphcache::parse(sample.key_expr()) {
                            tracing::trace!("topic: {}, rostype: {}, hash: {}, qos: {}", ke.topic(), ke.rostype(), ke.hash(), ke.qos());
                            // TODO: Should we deal with namespace?
                            // Transform the topic name from % to /, e.g. %camera%image_raw -> /camera/image_raw
                            let original_topic = ke.topic().replace("%", "/").to_string();
                            if let Ok(compare_key) = keyexpr::new(&original_topic[1..]) {
                                // Filter the topic which is not recorded
                                let any_match = topics.iter().any(|t| t.includes(compare_key));
                                if !any_match {
                                    tracing::debug!("topic {} is not in the recorded list, skipping...", ke.topic());
                                    continue;
                                }
                            } else {
                                tracing::warn!("Something wrong with the topic name: {original_topic}");
                                continue;
                            }
                            let rostype = utils::dds_type_to_ros_type(ke.rostype());

                            // TODO: We need to send a query to get the data (ROS message type definition)
                            // Check schemas
                            let schema_id = match schemas_map.get(&rostype) {
                                Some(id) => *id,
                                None => {
                                    let dummy_data = "TODO".as_bytes();
                                    let id = out.add_schema(&rostype, "ros2msg", dummy_data)?;
                                    tracing::debug!("Adding new schema for rostype: {} and id: {id}", rostype);
                                    schemas_map.insert(rostype, id);
                                    id
                                }
                            };
                            // Check channels
                            if !channels_map.contains_key(&ke.topic().to_string()) {
                                let metadata = BTreeMap::from([
                                    ("offered_qos_profiles".to_string(), utils::zenoh_qos_to_string(ke.qos())?),
                                    ("topic_type_hash".to_string(), ke.hash().to_string()),
                                ]);
                                let channel_id = out.add_channel(
                                    schema_id,
                                    &original_topic,
                                    "cdr",
                                    &metadata,
                                )?;
                                channels_map.insert(original_topic.to_string(), channel_id);
                                tracing::debug!("Adding new channel for topic: {original_topic} with id: {channel_id}");
                            }
                        }
                    } else {
                        tracing::error!("Error receiving liveliness sample");
                        break;
                    }
                },
                _ = &mut stop_rx => {
                    tracing::debug!("Stop signal received.");
                    break;
                }
            }
        }

        // TODO: Write the metadata again at the final stage
        out.write_metadata(&Metadata {
            name: "rosbag2".to_string(),
            metadata: BTreeMap::from([(
                "serialized_metadata".to_string(),
                metadata.to_yaml_string()?,
            )]),
        })?;
        out.finish()?;
        tracing::debug!(
            "Stopped recording topic '{:?}' on domain {}",
            topics,
            domain
        );
        Ok(())
    }

    async fn stop(self) {
        let _ = self.stop_tx.send(());
        let _ = self.handle.await;
    }

    fn is_finished(&self) -> bool {
        self.handle.is_finished()
    }
}
