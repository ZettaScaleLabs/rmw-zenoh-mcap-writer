//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use zenoh::{
    Session,
    key_expr::format::{kedefine, keformat},
};

kedefine!(
    pub(crate) ke_rostopic: "${domain:*}/${topic:*}/${remaining:**}",
);

pub struct RecorderHandler {
    session: Session,
    task: Option<RecordTask>,
}

impl RecorderHandler {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            task: None,
        }
    }

    pub fn start(&mut self, topic: String, domain: u32) -> Result<String> {
        if self.task.is_some() {
            return Err(anyhow!("Recording task is already running"));
        }
        let record_task = RecordTask::new(self.session.clone(), topic, domain);
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
            tokio::spawn(async move {
                task.stop().await;
            });
            Ok("Recording stopped".to_string())
        } else {
            Err(anyhow!("No recording task is running"))
        }
    }
}

struct RecordTask {
    topic: String,
    domain: u32,
    handle: tokio::task::JoinHandle<()>,
}

impl RecordTask {
    fn new(session: Session, topic: String, domain: u32) -> Self {
        let topic_clone = topic.clone();
        let handle = tokio::spawn(async move {
            tracing::info!("Started recording topic '{}' on domain {}", topic, domain);
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
