
FROM ros:kilted-ros-core

# Install necessary packages
RUN apt-get update && \
    apt-get install -y \
        --no-install-recommends \
        curl \
        wget \
        git \
        tmux

# Install ROS necessary packages
RUN apt-get update && \
    apt-get install -y --only-upgrade ros-kilted-* && \
    apt-get install -y \
      ros-kilted-rmw-zenoh-cpp \
      ros-kilted-demo-nodes-cpp

# Enable bash complete
RUN rm /etc/apt/apt.conf.d/docker-clean
RUN apt-get update && \
    apt-get install -y bash-completion
