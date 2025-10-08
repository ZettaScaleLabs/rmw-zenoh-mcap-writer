//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use std::{collections::BTreeMap, fs, io::BufWriter};
use tokio::sync::oneshot;

use anyhow::{Result, anyhow};
use chrono::Local;
use mcap::{Writer, records::MessageHeader};
use zenoh::{
    Session,
    key_expr::format::{kedefine, keformat},
};

kedefine!(
    pub(crate) ke_rostopic: "${domain:*}/${topic:*}/${remaining:**}",
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
        let handle = tokio::spawn(async move {
            tracing::info!("Started recording topic '{}' on domain {}", topic, domain);
            let options = mcap::WriteOptions::default()
                .profile("ros2")
                .calculate_chunk_crcs(false)
                .compression(None);
            let mut out =
                Writer::with_options(BufWriter::new(fs::File::create(fullpath).unwrap()), options)
                    .unwrap();

            let key_expr = keformat!(
                ke_rostopic::formatter(),
                domain = domain,
                topic = &topic,
                remaining = "**"
            )
            .unwrap();
            tracing::info!("Subscribing to key expression: {}", key_expr);
            let subscriber = session.declare_subscriber(&key_expr).await.unwrap();
            loop {
                tokio::select! {
                    sample = subscriber.recv_async() => {
                        if let Ok(sample) = sample {
                            tracing::info!(
                                "Received sample on topic '{}': {:?}",
                                sample.key_expr(),
                                sample.payload()
                            );
                            // TODO: Check if the channel exists, if not, create it
                            let channel_id = out
                                .add_channel(
                                    0,
                                    "cool stuff",
                                    "application/octet-stream",
                                    &BTreeMap::new(),
                                )
                                .unwrap();
                            // TODO: Check if the schema exists, if not, send a query to get type and create it
                            // TODO: Write the sample to the MCAP file
                            out.write_to_known_channel(
                                &MessageHeader {
                                    channel_id,
                                    sequence: 25,
                                    log_time: 6,
                                    publish_time: 24,
                                },
                                &[1, 2, 3],
                            )
                            .unwrap();
                        } else {
                            tracing::error!("Error receiving sample");
                            break;
                        }
                    },
                    _ = &mut stop_rx => {
                        tracing::info!("Stop signal received.");
                        break;
                    }
                }
            }
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
