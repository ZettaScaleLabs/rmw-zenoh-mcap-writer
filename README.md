# rmw-zenoh-mcap-writer

The MCAP writer is able to subscribe to all or a subset of ROS 2 topics and write the received messages into a MCAP file with a rosbag-compatible format.

## API design

When `rmw-zenoh-mcap-writer` is running, we can send an API request to start or stop recording.
You can use either the Zenoh API or the HTTP API.

* key: `@mcap/writer`
* value is the JSON format
  * The topic and domain fields are optional field.

```json
{
    // The recording status: can be "start" or "stop"
    "status": "start",
    // (optional) The recorded ROS topic: all topic by default
    "topic": "chatter",
    // (optional) The ROS domain ID: 0 by default
    "domain": "2"
}
```

### HTTP API

`rmw-zenoh-mcap-writer` loads the REST plugin, so we can send HTTP request to start / stop recording.
The URL should always be `http://your_host:8000/@mcap/writer`.

Take some examples:

* Recording the ROS topic with ROS Domain ID 2

```bash
curl -X PUT 'http://localhost:8000/@/*/@mcap/writer' -d '{"status": "start", "topic": "chatter", "domain": "2"}'
```

* Recording all ROS topic with the default ROS Domain ID 0

```bash
curl -X PUT 'http://localhost:8000/@/*/@mcap/writer' -d '{"status": "start"}'
```

* Stop recording

```bash
curl -X PUT 'http://localhost:8000/@/*/@mcap/writer' -d '{"status": "stop"}'
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
curl -X PUT 'http://localhost:8000/@/*/@mcap/writer' -d '{"status": "start"}'
```

* Run ROS 2 talker (TODO: Using test Docker image)

* Send request to stop recording the ROS 2 topic

```bash
curl -X PUT 'http://localhost:8000/@/*/@mcap/writer' -d '{"status": "stop"}'
```

* TODO: Replay the recorded MCAP
