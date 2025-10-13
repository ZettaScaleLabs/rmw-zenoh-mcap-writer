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
    key_expr::format::{kedefine, keformat},
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

    pub fn start(&mut self, topic: String, domain: u32) -> Result<String> {
        if self.task.is_some() {
            return Err(anyhow!("Recording task is already running"));
        }
        let record_task = RecordTask::new(self.session.clone(), self.path.clone(), topic, domain);
        self.task = Some(record_task);
        Ok("Recording started".to_string())
    }

    pub fn status(&self) -> String {
        if self.task.is_some() {
            "recording".to_string()
        } else {
            "stopped".to_string()
        }
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
    fn new(session: Session, path: String, topic: String, domain: u32) -> Self {
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let filename = format!("rosbag2_{}.mcap", Local::now().format("%Y_%m_%d_%H_%M_%S"));
        let fullpath = format!("{}/{}", path, filename);
        let filename_clone = filename.clone();
        let handle = tokio::spawn(async move {
            tracing::info!("Started recording topic '{}' on domain {}", topic, domain);
            // create a hashmap to store schema (String => u16)
            let mut schemas_map: BTreeMap<String, u16> = BTreeMap::new();
            let mut channels_map: BTreeMap<String, u16> = BTreeMap::new();
            let options = mcap::WriteOptions::default()
                .profile("ros2")
                .calculate_chunk_crcs(false)
                .compression(None);
            let mut out =
                Writer::with_options(BufWriter::new(fs::File::create(fullpath).unwrap()), options)
                    .unwrap();
            // TODO: Write the metadata
            let metadata = utils::BagMetadata::new(&filename_clone);
            out.write_metadata(&Metadata {
                name: "rosbag2".to_string(),
                metadata: BTreeMap::from([(
                    "serialized_metadata".to_string(),
                    metadata.to_yaml_string(),
                )]),
            })
            .unwrap();

            // Subscribe to the topic
            let key_expr = keformat!(
                ke_rostopic::formatter(),
                domain = domain,
                topic = &topic,
                rostype = "*",
                hash = "*",
            )
            .unwrap();
            tracing::info!("Subscribing to key expression: {}", key_expr);
            let subscriber = session.declare_subscriber(&key_expr).await.unwrap();

            // Subscribe to the liveliness
            let key_expr = keformat!(
                ke_graphcache::formatter(),
                domain = domain,
                zid = "*",
                node = "*",
                entity = "*",
                node_name = "*",
                topic = &topic,
                rostype = "*",
                hash = "*",
                qos = "*",
            )
            .unwrap();
            tracing::info!("Subscribing to liveliness key expression: {}", key_expr);
            let liveliness_subscriber = session
                .liveliness()
                .declare_subscriber(&key_expr)
                .history(true)
                .await
                .unwrap();
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
                                tracing::info!("topic: {}, rostype: {}, hash: {}", ke.topic(), ke.rostype(), ke.hash());
                                let topic = "/".to_string() + &ke.topic();  // The topic requires the leading '/'
                                if let Some(channel_id) = channels_map.get(&topic) {
                                    tracing::info!("Found existing channel_id: {}", channel_id);
                                    let current_time = Utc::now().timestamp_nanos_opt().unwrap() as u64;
                                    out.write_to_known_channel(
                                        &MessageHeader {
                                            channel_id: *channel_id,
                                            sequence: 0,  // It should be 0 if we don't use the sequence
                                            log_time: current_time,  // Receive timestamp
                                            publish_time: current_time,  // TODO: Parse the Zenoh attachment
                                        },
                                        sample.payload().to_bytes().as_ref(),
                                    )
                                    .unwrap();
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
                                tracing::info!("rostype: {}, hash: {}, qos: {}", ke.rostype(), ke.hash(), ke.qos());
                                let rostype = utils::dds_type_to_ros_type(&ke.rostype());

                                // TODO: We need to send a query to get the data (ROS message type definition)
                                let schema_id = match schemas_map.get(&rostype) {
                                    Some(id) => *id,
                                    None => {
                                        let dummy_data = "TODO".as_bytes();
                                        let id = out.add_schema(&rostype, "ros2msg", dummy_data).unwrap();
                                        tracing::info!("Adding new schema for rostype: {} and id: {id}", rostype);
                                        schemas_map.insert(rostype, id);
                                        id
                                    }
                                };
                                if !channels_map.contains_key(&ke.topic().to_string()) {
                                    let metadata = BTreeMap::from([
                                        ("offered_qos_profiles".to_string(), utils::zenoh_qos_to_string(ke.qos())),
                                        ("topic_type_hash".to_string(), ke.hash().to_string()),
                                    ]);
                                    // TODO: Should we deal with namespace?
                                    // Transform the topic name from % to /, e.g. %camera%image_raw -> /camera/image_raw
                                    let topic = ke.topic().replace('%', "/").to_string();
                                    let channel_id = out.add_channel(
                                        schema_id,
                                        &topic,
                                        "cdr",
                                        &metadata,
                                    ).unwrap();
                                    channels_map.insert(topic.to_string(), channel_id);
                                    tracing::info!("Adding new channel for topic: {topic} with id: {channel_id}");
                                }
                            }
                        } else {
                            tracing::error!("Error receiving liveliness sample");
                            break;
                        }
                    },
                    _ = &mut stop_rx => {
                        tracing::info!("Stop signal received.");
                        break;
                    }
                }
            }

            // TODO: Write the metadata again at the final stage
            out.write_metadata(&Metadata {
                name: "rosbag2".to_string(),
                metadata: BTreeMap::from([(
                    "serialized_metadata".to_string(),
                    metadata.to_yaml_string(),
                )]),
            })
            .unwrap();
            out.finish().unwrap();
            tracing::info!("Stopped recording topic '{}' on domain {}", topic, domain);
        });

        Self {
            handle,
            stop_tx,
            filename,
        }
    }

    async fn stop(self) {
        let _ = self.stop_tx.send(());
        let _ = self.handle.await;
    }
}
