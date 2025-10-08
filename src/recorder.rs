//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use std::{collections::BTreeMap, fs, io::BufWriter};

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
    topic: String,
    domain: u32,
    handle: tokio::task::JoinHandle<()>,
    filename: String,
}

impl RecordTask {
    fn new(session: Session, path: String, topic: String, domain: u32) -> Self {
        let topic_clone = topic.clone();
        let filename = format!("rosbag2_{}.mcap", Local::now().format("%Y_%m_%d_%H_%M_%S"));
        let fullpath = format!("{}/{}", path, filename);
        let handle = tokio::spawn(async move {
            tracing::info!("Started recording topic '{}' on domain {}", topic, domain);
            let options = mcap::WriteOptions::default()
                .calculate_chunk_crcs(false)
                .compression(None);
            let mut out =
                Writer::with_options(BufWriter::new(fs::File::create(fullpath).unwrap()), options)
                    .unwrap();
            let channel_id = out
                .add_channel(
                    0,
                    "cool stuff",
                    "application/octet-stream",
                    &BTreeMap::new(),
                )
                .unwrap();
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
            out.finish().unwrap();

            let key_expr = keformat!(
                ke_rostopic::formatter(),
                domain = domain,
                topic = &topic,
                remaining = "**"
            )
            .unwrap();
            tracing::info!("Subscribing to key expression: {}", key_expr);
            let subscriber = session.declare_subscriber(&key_expr).await.unwrap();
            while let Ok(sample) = subscriber.recv_async().await {
                tracing::info!(
                    "Received sample on topic '{}': {:?}",
                    sample.key_expr(),
                    sample.payload()
                );
            }
            tracing::info!("Stopped recording topic '{}' on domain {}", topic, domain);
        });

        Self {
            topic: topic_clone,
            domain,
            handle,
            filename,
        }
    }

    async fn stop(self) {
        // TODO: Have a more graceful shutdown
        self.handle.abort();
        tracing::info!(
            "Aborted recording task for topic '{}' on domain {}",
            self.topic,
            self.domain
        );
    }
}
