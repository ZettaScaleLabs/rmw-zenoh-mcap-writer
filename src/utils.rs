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
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Result, anyhow};
use chrono::Duration;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use zenoh::key_expr::format::kedefine;

kedefine!(
    // The format of ROS topic is Domain/Topic/ROSType/Hash
    // However, there might be several chunks splitted by `/` in Topic,
    // so we can't parse the key expression easily.
    // Instead, we will parse it with a customized function.
    pub(crate) ke_sub_rostopic: "${domain:*}/${topic:**}/${type_name:*}/${type_hash:*}",
    // There is no similar issue liveliness token, because `/` is transformed into `%` in the key expression.
    pub(crate) ke_graphcache: "@ros2_lv/${domain:*}/${zid:*}/${node:*}/${entity:*}/${entity_kind:*}/${enclave:*}/${namespace:*}/${node_name:*}/${topic:*}/${rostype:*}/${hash:*}/${qos:*}",
);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct NanoDuration {
    nanoseconds: u128,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct StartingTime {
    nanoseconds_since_epoch: u128,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct BagMetadata {
    version: u32,
    storage_identifier: String,
    relative_file_paths: Vec<String>,
    // TODO: Need to have a full FileInformation struct
    files: Vec<String>,
    duration: NanoDuration,
    starting_time: StartingTime,
    message_count: u64,
    // TODO: Need to have a full TopicInformation struct
    topics_with_message_count: Vec<String>,
    compression_format: String,
    compression_mode: String,
    // TODO: Empty should be ~. Should we use HashMap<String, String>?
    custom_data: String,
    ros_distro: String,
}

impl BagMetadata {
    pub(crate) fn new(filename: &String, ros_distro: String) -> Result<Self> {
        Ok(BagMetadata {
            version: 9,
            storage_identifier: "mcap".to_string(),
            relative_file_paths: vec![filename.to_owned()],
            files: vec![],
            duration: NanoDuration { nanoseconds: 0 },
            starting_time: StartingTime {
                nanoseconds_since_epoch: {
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
                    now.as_nanos()
                },
            },
            message_count: 0,
            topics_with_message_count: vec![],
            compression_format: "".to_string(),
            compression_mode: "".to_string(),
            custom_data: "~".to_string(),
            ros_distro,
        })
    }

    pub(crate) fn to_yaml_string(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}

/// Transform a DDS type name to a ROS type name
/// For example, from `std_msgs::msg::dds_::String_` to `std_msgs/msg/String`
/// Refer to: https://github.com/ros2/rosidl_dds/blob/772632eb729ab48f368a0862659224be80caf56b/rosidl_generator_dds_idl/rosidl_generator_dds_idl/__init__.py#L73
// TODO: Have a full mapping function
pub(crate) fn dds_type_to_ros_type(dds_type: &str) -> String {
    // Remove `dds_::` and replace all `::` with `/`
    let mut ros_type = dds_type.replace("dds_::", "").replace("::", "/");
    // remove _ in the final part of the string
    ros_type.pop();
    ros_type
}

/// Parse the key_expression "Domain/Topic/Rostype/Hash"
pub(crate) fn parse_subscription_ros_keyepxr(
    input: &str,
) -> Result<(String, String, String, String)> {
    // Split the parts
    let parts: Vec<&str> = input.split('/').collect();
    if parts.len() < 4 {
        return Err(anyhow!(
            "Wrong format: It should be Domain/Topic/Rostype/Hash"
        ));
    }

    let domain = parts[0];
    let hash = parts[parts.len() - 1];
    let rostype = parts[parts.len() - 2];
    // Assemble topic
    let topic_parts = &parts[1..parts.len() - 2];
    let topic = topic_parts.join("/");

    Ok((
        domain.to_string(),
        topic,
        rostype.to_string(),
        hash.to_string(),
    ))
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(remote = "Duration")]
pub(crate) struct DurationDef {
    #[serde(rename = "sec", getter = "Duration::num_seconds")]
    secs: i64,
    #[serde(rename = "nsec", getter = "Duration::subsec_nanos")]
    nanos: i32,
}

fn default_duration() -> Duration {
    Duration::nanoseconds(i64::MAX)
}

#[derive(Debug, Serialize)]
pub(crate) struct QoSProfile {
    pub(crate) history: String,
    pub(crate) depth: Option<u32>,
    pub(crate) reliability: String,
    pub(crate) durability: String,
    #[serde(with = "DurationDef")]
    pub(crate) deadline: Duration,
    #[serde(with = "DurationDef")]
    pub(crate) lifespan: Duration,
    pub(crate) liveliness: String,
    #[serde(with = "DurationDef")]
    pub(crate) liveliness_lease_duration: Duration,
    pub(crate) avoid_ros_namespace_conventions: bool,
}

impl Default for QoSProfile {
    fn default() -> Self {
        QoSProfile {
            history: "keep_last".to_string(),
            depth: Some(10),
            reliability: "reliable".to_string(),
            durability: "volatile".to_string(),
            deadline: default_duration(),
            lifespan: default_duration(),
            liveliness: "automatic".to_string(),
            liveliness_lease_duration: default_duration(),
            avoid_ros_namespace_conventions: false,
        }
    }
}

pub(crate) fn parse_zenoh_qos(zenoh_qos: &str) -> Result<QoSProfile> {
    let mut qos_profile = QoSProfile::default();
    let parts: Vec<&str> = zenoh_qos.split(':').collect();
    // Reliable
    if parts[0] == "2" {
        qos_profile.reliability = "best_effort".to_string();
    } else {
        qos_profile.reliability = "reliable".to_string();
    }
    // Durability
    if parts[1] == "1" {
        qos_profile.durability = "transient_local".to_string();
    } else {
        qos_profile.durability = "volatile".to_string();
    }
    // History
    let subparts = parts[2].split(',').collect::<Vec<&str>>();
    if subparts[0] == "2" {
        qos_profile.history = "keep_all".to_string();
    } else {
        qos_profile.history = "keep_last".to_string();
        qos_profile.depth = Some(subparts[1].parse::<u32>()?);
    }
    // Deadline
    let subparts = parts[3].split(',').collect::<Vec<&str>>();
    if !subparts[0].is_empty() && !subparts[1].is_empty() {
        qos_profile.deadline = Duration::seconds(subparts[0].parse::<i64>()?)
            + Duration::nanoseconds(subparts[1].parse::<i32>()? as i64);
    }
    // Lifespan
    let subparts = parts[4].split(',').collect::<Vec<&str>>();
    if !subparts[0].is_empty() && !subparts[1].is_empty() {
        qos_profile.lifespan = Duration::seconds(subparts[0].parse::<i64>()?)
            + Duration::nanoseconds(subparts[1].parse::<i32>()? as i64);
    }
    // Liveliness
    let subparts = parts[5].split(',').collect::<Vec<&str>>();
    if subparts[0] == "2" {
        qos_profile.liveliness = "manual_by_topic".to_string();
        if !subparts[0].is_empty() && !subparts[1].is_empty() {
            qos_profile.liveliness_lease_duration = Duration::seconds(subparts[1].parse::<i64>()?)
                + Duration::nanoseconds(subparts[2].parse::<i32>()? as i64);
        }
    } else {
        qos_profile.liveliness = "automatic".to_string();
    }

    Ok(qos_profile)
}

/// Transform Zenoh QoS into a string
pub(crate) fn zenoh_qos_to_string(zenoh_qos: &str) -> Result<String> {
    Ok(serde_yaml::to_string(&vec![parse_zenoh_qos(zenoh_qos)?])?)
}

static GLOBAL_COUNTER: Lazy<AtomicU32> = Lazy::new(|| AtomicU32::new(1));
/// Get a new entity ID by increasing the number
pub(crate) fn get_entity_id() -> u32 {
    GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst)
}

// Add test
#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("std_msgs::msg::dds_::String_", "std_msgs/msg/String"; "String")]
    fn test_dds_type_to_ros_type(dds_type: &str, ros_type: &str) {
        let transformed_ros_type = dds_type_to_ros_type(dds_type);
        assert_eq!(ros_type, transformed_ros_type);
    }
}
