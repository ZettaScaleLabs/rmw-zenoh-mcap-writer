//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use anyhow::{Result, anyhow};
use zenoh::Session;

const DEFAULT_ROS_DISTRO: &str = "kilted";
const ROS_DISTRO_REGISTRY_KEY: &str = "@ros2_env/ROS_DISTRO";

async fn get_ros_distro_from_registry(zsession: Session) -> Result<String> {
    let replies = zsession
        .get(ROS_DISTRO_REGISTRY_KEY)
        .await
        .map_err(|e| anyhow!("Unable to send query to get ROS_DISTRO: {e}"))?;
    let reply = replies
        .recv_async()
        .await
        .map_err(|e| anyhow!("Unable to get the reply of ROS_DISTRO: {e}"))?;
    let sample = reply.result().map_err(|e| {
        anyhow!("Unable to get the result while sending query to get ROS_DISTRO: {e}")
    })?;
    let ros_distro = sample.payload().try_to_string()?;
    Ok(ros_distro.to_string())
}

pub(crate) async fn get_ros_distro(zsession: Session) -> String {
    get_ros_distro_from_registry(zsession)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to retrieve ROS_DISTRO, using default. Error: {}", e);
            DEFAULT_ROS_DISTRO.to_string()
        })
}

pub(crate) async fn get_ros_msg_data_from_registry(
    zsession: Session,
    rostype: &String,
) -> Result<String> {
    let key = format!("@ros2_types/{}?format=Mcap", rostype);
    let replies = zsession
        .get(key)
        .await
        .map_err(|e| anyhow!("Unable to send query to get ROS type: {e}"))?;
    let reply = replies
        .recv_async()
        .await
        .map_err(|e| anyhow!("Unable to get the reply of ROS type: {e}"))?;
    let sample = reply.result().map_err(|e| {
        anyhow!("Unable to get the result while sending query to get ROS type: {e}")
    })?;
    let ros_data = sample.payload().try_to_string()?;
    Ok(ros_data.to_string())
}

pub(crate) async fn get_ros_msg_data(zsession: Session, rostype: &String) -> Result<String> {
    get_ros_msg_data_from_registry(zsession, rostype).await
}
