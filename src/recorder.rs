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
use std::{collections::BTreeMap, fs, io::BufWriter};

use anyhow::{Result, anyhow};
use chrono::{Local, Utc};
use mcap::{Writer, records::MessageHeader, write::Metadata};
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot,
};
use zenoh::{
    Session,
    key_expr::{
        KeyExpr, OwnedKeyExpr,
        format::{kedefine, keformat},
        keyexpr,
    },
    liveliness::LivelinessToken,
    pubsub::Subscriber,
    sample::{Sample, SampleKind},
};
use zenoh_ext::ZDeserializer;

use crate::registry;
use crate::utils;

const CHANNEL_SIZE: usize = 2048;
const NODE_ID: u32 = 0;
const NODE_NAME: &str = "mcap_writer";

kedefine!(
    // The format of ROS topic is Domain/Topic/ROSType/Hash
    // However, there might be several chunks splitted by `/` in Topic,
    // so we can't parse the key expression easily.
    // Instead, we will parse it with a customized function.
    pub(crate) ke_sub_rostopic: "${domain:*}/${topic:**}/${type_name:*}/${type_hash:*}",
    // There is no similar issue liveliness token, because `/` is transformed into `%` in the key expression.
    pub(crate) ke_graphcache: "@ros2_lv/${domain:*}/${zid:*}/${node:*}/${entity:*}/${entity_kind:*}/${enclave:*}/${namespace:*}/${node_name:*}/${topic:*}/${rostype:*}/${hash:*}/${qos:*}",
);

pub(crate) struct RecorderHandler {
    session: Session,
    path: String,
    task: Option<RecordTask>,
}

impl RecorderHandler {
    pub(crate) fn new(session: Session, path: String) -> Self {
        Self {
            session,
            path,
            task: None,
        }
    }

    pub(crate) fn start(
        &mut self,
        topic: String,
        domain: &str,
        ros_distro: String,
    ) -> Result<String> {
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

    pub(crate) fn status(&mut self) -> String {
        if let Some(ref task) = self.task
            && !task.is_finished()
        {
            return "recording".to_string();
        }
        self.task.take(); // Drop the task
        "stopped".to_string()
    }

    pub(crate) fn stop(&mut self) -> Result<String> {
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
        tracing::debug!("Create a recorded mcap file with name: {filename}");
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

    async fn create_a_topic_recorder(
        session: Session,
        key_expr: &KeyExpr<'static>,
        topic: &String,
        tx: Sender<Sample>,
        topic_recorder_list: &mut Vec<(Subscriber<()>, LivelinessToken)>,
    ) -> Result<()> {
        // Parse the key_expr
        let ke = ke_graphcache::parse(key_expr)
            .map_err(|e| anyhow!("Unable to parse the key expression: {e}"))?;

        // Create subscriber
        let key_expr = keformat!(
            ke_sub_rostopic::formatter(),
            domain = ke.domain(),
            topic,
            type_name = "*",
            type_hash = "*"
        )
        .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
        let key_expr_cloned = key_expr.clone();
        tracing::debug!("Subscribing to key expression: {}", key_expr);
        let subscriber = session
            .declare_subscriber(&key_expr)
            .callback(move |sample| {
                // Put data to TX
                if let Err(e) = tx.try_send(sample) {
                    tracing::error!(
                        "Failed to put data (from {key_expr_cloned}) into the mpsc channel: {e}",
                    );
                }
            })
            .await
            .map_err(|e| anyhow!("Unable to declare the subscriber: {e}"))?;

        // Create liveliness token
        let key_expr = keformat!(
            ke_graphcache::formatter(),
            domain = ke.domain(),
            zid = session.id().zid(),
            node = NODE_ID,
            entity = utils::get_entity_id(),
            entity_kind = "MS",
            enclave = ke.enclave(),
            namespace = ke.namespace(),
            node_name = NODE_NAME,
            topic = ke.topic(),
            rostype = ke.rostype(),
            hash = ke.hash(),
            qos = ke.qos(),
        )
        .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
        let token = session
            .liveliness()
            .declare_token(&key_expr)
            .await
            .map_err(|e| anyhow!("Unable to declare the livelness token: {e}"))?;

        // Store the subscriber and liveliness tokens to avoid dropping
        topic_recorder_list.push((subscriber, token));

        Ok(())
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
        // Create a hashmap to store schema (String => u16)
        let mut schemas_map: BTreeMap<String, u16> = BTreeMap::new();
        let mut channels_map: BTreeMap<String, u16> = BTreeMap::new();
        // Create a mpsc unbounded channel to write data
        let mut topic_recorder_list: Vec<(Subscriber<()>, LivelinessToken)> = Vec::new();
        let (tx, mut rx) = mpsc::channel(CHANNEL_SIZE);

        // MCAP Writer
        let options = mcap::WriteOptions::default()
            .profile("ros2")
            .calculate_chunk_crcs(false)
            .compression(None);
        let mut out = Writer::with_options(BufWriter::new(fs::File::create(fullpath)?), options)?;

        // Write the metadata
        // TODO: the metadata should be updated
        let metadata = utils::BagMetadata::new(&filename, ros_distro)?;
        out.write_metadata(&Metadata {
            name: "rosbag2".to_string(),
            metadata: BTreeMap::from([(
                "serialized_metadata".to_string(),
                metadata.to_yaml_string()?,
            )]),
        })?;

        // Subscribe to the liveliness
        let key_expr = keformat!(
            ke_graphcache::formatter(),
            domain = domain,
            zid = "*",
            node = "*",
            entity = "*",
            entity_kind = "MP",
            enclave = "*",
            namespace = "*",
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
                            // Transform the topic name from % to /, e.g. %camera%image_raw -> /camera/image_raw -> camera/image_raw
                            let original_topic = ke.topic().replace("%", "/").to_string();
                            let original_topic_no_leading = original_topic[1..].to_string();
                            if let Ok(compare_key) = keyexpr::new(&original_topic_no_leading) {
                                // Filter the topic which is not recorded
                                let any_match = topics.iter().any(|t| t.includes(compare_key));
                                if !any_match {
                                    tracing::debug!("topic {} (key: {}) is not in the recorded list, skipping...", ke.topic(), original_topic);
                                    continue;
                                }
                            } else {
                                tracing::warn!("Something wrong with the topic name: {original_topic}");
                                continue;
                            }

                            // Create subscribers with a callback to put data into the channel
                            RecordTask::create_a_topic_recorder(session.clone(), sample.key_expr(), &original_topic_no_leading, tx.clone(), &mut topic_recorder_list).await?;

                            // Check schemas
                            let rostype = utils::dds_type_to_ros_type(ke.rostype());
                            let schema_id = match schemas_map.get(&rostype) {
                                Some(id) => *id,
                                None => {
                                    match registry::get_ros_msg_data(session.clone(), &rostype).await {
                                        Ok(ros_msg_data) => {
                                            let id = out.add_schema(&rostype, "ros2msg", ros_msg_data.as_bytes())?;
                                            tracing::debug!("Adding new schema for id: {id}, rostype: {rostype}, data: {ros_msg_data}");
                                            schemas_map.insert(rostype, id);
                                            id
                                        },
                                        Err(e) => {
                                            tracing::warn!("Unable to get the ROS message data of {rostype} from the type registry. Use empty instead. Error: {e}");
                                            let id = out.add_schema(&rostype, "ros2msg", "".as_bytes())?;
                                            tracing::debug!("Adding new schema for id: {id}, rostype: {rostype}, data: ''");
                                            schemas_map.insert(rostype, id);
                                            id
                                        }
                                    }
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
                        } else {
                            tracing::warn!("Something wrong when parsing the liveliness token name: {}", sample.key_expr());
                        }
                    } else {
                        tracing::error!("Error receiving liveliness sample");
                        break;
                    }
                },
                // Receive from tx and store it into MCAP
                Some(sample) = rx.recv() => {
                    tracing::trace!(
                        "Received sample on topic '{}': {:?}",
                        sample.key_expr(),
                        sample.payload()
                    );
                    if let Ok((_domain, topic, rostype, hash)) = utils::parse_subscription_ros_keyepxr(sample.key_expr()) {
                        tracing::debug!("topic: {}, rostype: {}, hash: {}", topic, rostype, hash);

                        // Deserialize the attachment
                        let current_time = Utc::now().timestamp_nanos_opt().ok_or(anyhow!("Unable to get the current time"))? as u64;
                        let (sequence, publish_time) = if let Some(attachment) = sample.attachment() {
                            let mut deserializer = ZDeserializer::new(attachment);
                            // The sequence and publish_time are both i64, but we need to map them to u32 and u64
                            let sequence = deserializer.deserialize::<i64>().unwrap_or(0) as u32;
                            let publish_time = deserializer.deserialize::<u64>().unwrap_or(current_time);
                            (sequence, publish_time)
                        } else {
                            (0, current_time)
                        };

                        // Write the record into the file
                        let topic = "/".to_string() + &topic;  // The topic requires the leading '/'
                        if let Some(channel_id) = channels_map.get(&topic) {
                            tracing::debug!("Found existing channel_id: {}", channel_id);
                            out.write_to_known_channel(
                                &MessageHeader {
                                    channel_id: *channel_id,
                                    sequence,
                                    log_time: current_time,  // Receive timestamp
                                    publish_time,
                                },
                                sample.payload().to_bytes().as_ref(),
                            )?;
                        } else {
                            tracing::debug!("Skip the messages because there is no existing channel_id for topic: {}", topic);
                        }
                    } else {
                        tracing::warn!("Something wrong when parsing the topic name: {}", sample.key_expr());
                    }
                },
                _ = &mut stop_rx => {
                    tracing::debug!("Stop signal received.");
                    break;
                }
            }
        }

        // Cleanup the subscribers and liveliness tokens
        for (sub, token) in topic_recorder_list {
            let _ = sub.undeclare().await;
            let _ = token.undeclare().await;
        }

        // Write the metadata again at the final stage
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
