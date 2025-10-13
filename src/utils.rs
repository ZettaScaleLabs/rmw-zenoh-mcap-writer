//
// Copyright (c) 2025 ZettaScale Technology
// All rights reserved.
//
// This software is the confidential and proprietary information of ZettaScale Technology.
//
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
    // TODO: Empty should be ~
    //custom_data: HashMap<String, String>,
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
