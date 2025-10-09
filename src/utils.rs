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
