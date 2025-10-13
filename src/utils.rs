//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
use chrono::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct NanoDuration {
    nanoseconds: u128,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct StartingTime {
    nanoseconds_since_epoch: u128,
}

// TODO: Add more fields if necessary
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct BagMetadata {
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
    pub fn new(filename: &String) -> Self {
        BagMetadata {
            version: 9,
            storage_identifier: "mcap".to_string(),
            relative_file_paths: vec![filename.to_owned()],
            files: vec![],
            duration: NanoDuration { nanoseconds: 0 },
            starting_time: StartingTime {
                nanoseconds_since_epoch: {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap();
                    now.as_nanos()
                },
            },
            message_count: 0,
            topics_with_message_count: vec![],
            compression_format: "".to_string(),
            compression_mode: "".to_string(),
            custom_data: "~".to_string(),
            // TODO: Get the distro from registry
            ros_distro: "rolling".to_string(),
        }
    }

    pub fn to_yaml_string(&self) -> String {
        serde_yaml::to_string(self).unwrap()
    }
}

/// Transform a DDS type name to a ROS type name
/// For example, from `std_msgs::msg::dds_::String_` to `std_msgs/msg/String`
/// Refer to: https://github.com/ros2/rosidl_dds/blob/772632eb729ab48f368a0862659224be80caf56b/rosidl_generator_dds_idl/rosidl_generator_dds_idl/__init__.py#L73
// TODO: Have a full mapping function
pub fn dds_type_to_ros_type(dds_type: &str) -> String {
    // Remove `dds_::` and replace all `::` with `/`
    let mut ros_type = dds_type.replace("dds_::", "").replace("::", "/");
    // remove _ in the final part of the string
    ros_type.pop();
    ros_type
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(remote = "Duration")]
pub struct DurationDef {
    #[serde(rename = "sec", getter = "Duration::num_minutes")]
    secs: i64,
    #[serde(rename = "nsec", getter = "Duration::subsec_nanos")]
    nanos: i32,
}

fn default_duration() -> Duration {
    Duration::nanoseconds(i64::MAX)
}

#[derive(Debug, Serialize)]
pub struct QoSProfile {
    history: String,
    depth: Option<u32>,
    reliability: String,
    durability: String,
    #[serde(with = "DurationDef")]
    deadline: Duration,
    #[serde(with = "DurationDef")]
    lifespan: Duration,
    liveliness: String,
    #[serde(with = "DurationDef")]
    liveliness_lease_duration: Duration,
    avoid_ros_namespace_conventions: bool,
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

fn parse_zenoh_qos(zenoh_qos: &str) -> QoSProfile {
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
        qos_profile.depth = Some(subparts[1].parse::<u32>().unwrap());
    }
    // Deadline
    let subparts = parts[3].split(',').collect::<Vec<&str>>();
    if subparts[0] != "" && subparts[1] != "" {
        qos_profile.deadline = Duration::seconds(subparts[0].parse::<i64>().unwrap())
            + Duration::nanoseconds(subparts[1].parse::<i32>().unwrap() as i64);
    }
    // Lifespan
    let subparts = parts[4].split(',').collect::<Vec<&str>>();
    if subparts[0] != "" && subparts[1] != "" {
        qos_profile.lifespan = Duration::seconds(subparts[0].parse::<i64>().unwrap())
            + Duration::nanoseconds(subparts[1].parse::<i32>().unwrap() as i64);
    }
    // Liveliness
    let subparts = parts[5].split(',').collect::<Vec<&str>>();
    if subparts[0] == "2" {
        qos_profile.liveliness = "manual_by_topic".to_string();
        if subparts[0] != "" && subparts[1] != "" {
            qos_profile.liveliness_lease_duration =
                Duration::seconds(subparts[1].parse::<i64>().unwrap())
                    + Duration::nanoseconds(subparts[2].parse::<i32>().unwrap() as i64);
        }
    } else {
        qos_profile.liveliness = "automatic".to_string();
    }

    qos_profile
}

/// Transform Zenoh QoS into a string
pub fn zenoh_qos_to_string(zenoh_qos: &str) -> String {
    serde_yaml::to_string(&vec![parse_zenoh_qos(zenoh_qos)]).unwrap()
}
