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
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::BufWriter,
    time::Duration,
};

use anyhow::{Result, anyhow};
use chrono::{Local, Utc};
use mcap::{Writer, records::MessageHeader, write::Metadata};
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot,
};
use zenoh::{
    Session,
    key_expr::{OwnedKeyExpr, format::keformat, keyexpr},
    liveliness::LivelinessToken,
    sample::Sample,
};
use zenoh_ext::{
    AdvancedSubscriber, AdvancedSubscriberBuilderExt, HistoryConfig, RecoveryConfig, ZDeserializer,
};

use crate::{
    keyexpr_monitor::{KeyExprInfo, StorageKeyExprs},
    registry, utils,
};

const CHANNEL_SIZE: usize = 2048;
const NODE_ID: u32 = 0;
const NODE_NAME: &str = "mcap_writer";

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
        storage_key_exprs: StorageKeyExprs,
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
            storage_key_exprs,
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
        storage_key_exprs: StorageKeyExprs,
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
                storage_key_exprs,
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
        keyexpr_info: &KeyExprInfo,
        topic: &String,
        tx: Sender<Sample>,
        topic_recorder_hashmap: &mut HashMap<String, (AdvancedSubscriber<()>, LivelinessToken)>,
    ) -> Result<()> {
        // Only create a topic recorder if it doesn't exist
        if topic_recorder_hashmap.contains_key(topic) {
            tracing::debug!("The topic {topic} has already been recorded");
        } else {
            // Create subscriber
            let key_expr = keformat!(
                utils::ke_sub_rostopic::formatter(),
                domain = keyexpr_info.domain,
                topic,
                type_name = "*",
                type_hash = "*"
            )
            .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
            let key_expr_cloned = key_expr.clone();
            let qos_profile = utils::parse_zenoh_qos(&keyexpr_info.qos)?;
            // We use the advanced subscriber here for transient local topic
            let subscriber_builder = if qos_profile.durability == "transient_local" {
                tracing::debug!(
                    "Create a transient_local subscriber for the key expression: {}",
                    key_expr
                );
                let history_config = if let Some(depth) = qos_profile.depth {
                    HistoryConfig::default()
                        .detect_late_publishers()
                        .max_samples(depth as usize)
                } else {
                    HistoryConfig::default().detect_late_publishers()
                };
                let adv_sub = session
                    .declare_subscriber(&key_expr)
                    .subscriber_detection()
                    .query_timeout(Duration::MAX)
                    .history(history_config);
                if qos_profile.reliability == "reliable" {
                    adv_sub.recovery(RecoveryConfig::default().heartbeat())
                } else {
                    adv_sub
                }
            } else {
                tracing::debug!("Create a subscriber for the key expression: {}", key_expr);
                session.declare_subscriber(&key_expr).advanced()
            };
            let subscriber = subscriber_builder
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
                utils::ke_graphcache::formatter(),
                domain = keyexpr_info.domain,
                zid = session.id().zid(),
                node = NODE_ID,
                entity = utils::get_entity_id(),
                entity_kind = "MS",
                enclave = keyexpr_info.enclave.to_string(),
                namespace = keyexpr_info.namespace.to_string(),
                node_name = NODE_NAME,
                topic = keyexpr_info.topic.to_string(),
                rostype = keyexpr_info.rostype.to_string(),
                hash = keyexpr_info.hash.to_string(),
                qos = keyexpr_info.qos.to_string(),
            )
            .map_err(|e| anyhow!("Unable to format the key expression: {e}"))?;
            tracing::debug!(
                "Create a liveliness token for the key expression: {}",
                key_expr
            );
            let token = session
                .liveliness()
                .declare_token(&key_expr)
                .await
                .map_err(|e| anyhow!("Unable to declare the livelness token: {e}"))?;

            // Store the subscriber and liveliness tokens to avoid dropping
            topic_recorder_hashmap.insert(topic.to_owned(), (subscriber, token));
        }

        Ok(())
    }

    // TODO: We can optimize it later
    #[allow(clippy::too_many_arguments)]
    async fn process_hashset_key_exprs(
        storage_key_exprs: StorageKeyExprs,
        session: Session,
        topics: Vec<OwnedKeyExpr>,
        domain: u32,
        tx: Sender<Sample>,
        schemas_map: &mut BTreeMap<String, u16>,
        channels_map: &mut BTreeMap<String, u16>,
        topic_recorder_hashmap: &mut HashMap<String, (AdvancedSubscriber<()>, LivelinessToken)>,
        out: &mut Writer<BufWriter<File>>,
    ) -> Result<()> {
        for (key_expr, keyexpr_info) in storage_key_exprs.to_vec() {
            // Filter the mismatched domain ID
            if domain != keyexpr_info.domain {
                tracing::debug!(
                    "The domain doesn't match ({domain} != {}), skipping...",
                    keyexpr_info.domain
                );
                continue;
            }

            // Transform the topic name from /camera/image_raw to camera/image_raw
            let original_topic_no_leading = keyexpr_info.original_topic[1..].to_string();
            if let Ok(compare_key) = keyexpr::new(&original_topic_no_leading) {
                // Filter the topic which is not recorded
                let any_match = topics.iter().any(|t| t.includes(compare_key));
                if !any_match {
                    tracing::debug!(
                        "topic {} (key: {}) is not in the recorded list, skipping...",
                        keyexpr_info.original_topic,
                        keyexpr_info.topic,
                    );
                    continue;
                }
            } else {
                tracing::warn!(
                    "Something wrong with the topic name: {}",
                    keyexpr_info.original_topic
                );
                continue;
            }

            // Create subscribers with a callback to put data into the channel
            tracing::debug!(
                "Detect a new liveliness token we haven't had yet: {}",
                key_expr
            );
            RecordTask::create_a_topic_recorder(
                session.clone(),
                &keyexpr_info,
                &original_topic_no_leading,
                tx.clone(),
                topic_recorder_hashmap,
            )
            .await?;

            // Check schemas
            let rostype = utils::dds_type_to_ros_type(&keyexpr_info.rostype);
            let schema_id = match schemas_map.get(&rostype) {
                Some(id) => *id,
                None => match registry::get_ros_msg_data(session.clone(), &rostype).await {
                    Ok(ros_msg_data) => {
                        let id = out.add_schema(&rostype, "ros2msg", ros_msg_data.as_bytes())?;
                        tracing::debug!(
                            "Adding new schema for id: {id}, rostype: {rostype}, data: {ros_msg_data}"
                        );
                        schemas_map.insert(rostype, id);
                        id
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Unable to get the ROS message data of {rostype} from the type registry. Use empty instead. Error: {e}"
                        );
                        let id = out.add_schema(&rostype, "ros2msg", "".as_bytes())?;
                        tracing::debug!(
                            "Adding new schema for id: {id}, rostype: {rostype}, data: ''"
                        );
                        schemas_map.insert(rostype, id);
                        id
                    }
                },
            };

            // Check channels
            if !channels_map.contains_key(&keyexpr_info.topic) {
                let metadata = BTreeMap::from([
                    (
                        "offered_qos_profiles".to_string(),
                        utils::zenoh_qos_to_string(&keyexpr_info.qos)?,
                    ),
                    ("topic_type_hash".to_string(), keyexpr_info.hash),
                ]);
                let channel_id =
                    out.add_channel(schema_id, &keyexpr_info.original_topic, "cdr", &metadata)?;
                channels_map.insert(keyexpr_info.original_topic.clone(), channel_id);
                tracing::debug!(
                    "Adding new channel for topic: {} with id: {channel_id}",
                    keyexpr_info.original_topic
                );
            }
        }
        Ok(())
    }

    // TODO: We can optimize it later
    #[allow(clippy::too_many_arguments)]
    async fn write_mcap(
        storage_key_exprs: StorageKeyExprs,
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
        let mut topic_recorder_hashmap: HashMap<String, (AdvancedSubscriber<()>, LivelinessToken)> =
            HashMap::new();
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

        // Process the existing HashSet
        RecordTask::process_hashset_key_exprs(
            storage_key_exprs.clone(),
            session.clone(),
            topics.clone(),
            domain,
            tx.clone(),
            &mut schemas_map,
            &mut channels_map,
            &mut topic_recorder_hashmap,
            &mut out,
        )
        .await?;

        loop {
            let notified_future = storage_key_exprs.notified();
            tokio::select! {
                // Check if the KeyExprs inside HashSet are changed or not
                _ = notified_future => {
                    tracing::debug!("The liveliness token of the KeyExprs are changed. Process it again.");
                    RecordTask::process_hashset_key_exprs(
                        storage_key_exprs.clone(),
                        session.clone(),
                        topics.clone(),
                        domain,
                        tx.clone(),
                        &mut schemas_map,
                        &mut channels_map,
                        &mut topic_recorder_hashmap,
                        &mut out,
                    )
                    .await?;
                },
                // Receive from tx and store it into MCAP
                Some(sample) = rx.recv() => {
                    tracing::trace!(
                        "Received sample on topic '{}': {:?}",
                        sample.key_expr(),
                        sample.payload()
                    );
                    if let Ok((_domain, topic, rostype, hash)) = utils::parse_subscription_ros_keyepxr(sample.key_expr()) {
                        tracing::trace!("From the key expression: topic={topic}, rostype={rostype}, hash={hash}");

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
                            tracing::trace!("Write a new record: channel_id={channel_id}, sequence={sequence}, log_time={current_time}, publish_time={publish_time}");
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
                            tracing::warn!("Skip the messages because there is no existing channel_id for topic: {}", topic);
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
        for (_topic, (sub, token)) in topic_recorder_hashmap {
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
