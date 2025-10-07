use anyhow::{Result, anyhow};

pub struct RecorderHandler {
    task: Option<RecordTask>,
}

impl RecorderHandler {
    pub fn new() -> Self {
        Self { task: None }
    }

    pub fn start(&mut self, topic: String, domain: u32) -> Result<String> {
        if self.task.is_some() {
            return Err(anyhow!("Recording task is already running"));
        }
        let record_task = RecordTask::new(topic, domain);
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
    fn new(topic: String, domain: u32) -> Self {
        let topic_clone = topic.clone();
        let handle = tokio::spawn(async move {
            // Simulate recording task
            tracing::info!("Started recording topic '{}' on domain {}", topic, domain);
            // Here would be the actual recording logic
            // For demonstration, we just sleep
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            tracing::info!("Stopped recording topic '{}' on domain {}", topic, domain);
        });

        Self {
            topic: topic_clone,
            domain,
            handle,
        }
    }

    async fn stop(self) {
        // Here we would implement a proper shutdown of the recording task
        // For now, we just abort the task
        self.handle.abort();
        tracing::info!(
            "Aborted recording task for topic '{}' on domain {}",
            self.topic,
            self.domain
        );
    }
}
