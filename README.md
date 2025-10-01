# rmw-zenoh-mcap-writer

The MCAP writer is able to subscribe to all or a subset of ROS 2 topics and write the received messages into a MCAP file with a rosbag-compatible format.

## API design

When `rmw-zenoh-mcap-writer` is running, we can send an Zenoh Query to start or stop recording.
There are two kinds of Zenoh selectors for different purposes:

* `@mcap_writer/start`: Start to record the ROS 2 topic.
* `@mcap_writer/stop`: Stop recording the ROS 2 topic.

Here are some parameters support in the selector:

* `topic`: The recorded ROS topic. It will record all topics if not specified.
* `domain`: The ROS domain ID. It's 0 if not sepecified.

### HTTP API

`rmw-zenoh-mcap-writer` loads the REST plugin, so we can send HTTP request to start / stop recording.
The URL should always be `http://your_host:8000/<selector>`.

Take some examples:

* Recording all ROS topic with the default ROS Domain ID 0

```bash
curl -X GET 'http://localhost:8000/@mcap_writer/start'
```

* Recording the ROS topic with ROS Domain ID 2

```bash
curl -X GET 'http://localhost:8000/@mcap_writer/start?domain=2&topic=chatter'
```

* Stop recording

```bash
curl -X GET 'http://localhost:8000/@mcap_writer/stop'
```

## Build

### In a Docker image

TODO

### On your host

* Install [Rust Toolchains](https://doc.rust-lang.org/cargo/getting-started/installation.html)

* Build the project

```bash
cargo build --release
```

## Usage

* Run the `rmw-zenoh-mcap-writer` if you're not using the Docker image

```bash
./target/release/rmw-zenoh-mcap-writer
```

* Send request to start recording the ROS 2 topic

```bash
curl -X GET 'http://localhost:8000/@mcap_writer/start'
```

* Run ROS 2 talker (TODO: Using test Docker image)

* Send request to stop recording the ROS 2 topic

```bash
curl -X GET 'http://localhost:8000/@mcap_writer/stop'
```

* TODO: Replay the recorded MCAP
