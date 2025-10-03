# rmw-zenoh-mcap-writer

The MCAP writer is able to subscribe to all or a subset of ROS 2 topics and write the received messages into a MCAP file with a rosbag-compatible format.

## API design

When `rmw-zenoh-mcap-writer` is running, we can send an Zenoh Query to start or stop recording.
There are two kinds of Zenoh selectors for different purposes:

* `@mcap/writer/start`: Start to record the ROS 2 topic.
  * Return value:
  
  ```json
  {
    "result": "success",  // It can be either success or failure
  }
  ```

* `@mcap/writer/status`: The status of the recorder.

  ```json
  {
    "status": "recording",  // It can be either recording or stopped
  }
  ```

* `@mcap/writer/stop`: Stop recording the ROS 2 topic.
  
  ```json
  {
    "result": "success",                       // It can be either success or failure
    "filename": "rosbag2_2025_10_03-16_28_40", // The stored rosbag filename  
  }
  ```

Here are some parameters support in the selector (only valid for `@mcap/writer/start`):

* `topic`: The recorded ROS topic list.
  * You can have a list of ROS topics to be recorded here, separated by `,`.
  * You can use `*`, which means matches any set of characters except for `/`. For example, `/robot/1/*` can match `/robot/1/lidar`, `/robot/1/camera`, and so on.
  * You can use `**`, which means matches any set of characters including `/`. For example, `/robot/**` can match `robot/1/lidar`, `/robot/2/camera`, and so on.
  * By default, it's `*`, which means recording all ROS topics.
* `domain`: The ROS domain ID. It's 0 if not sepecified.

### HTTP API

`rmw-zenoh-mcap-writer` loads the REST plugin, so we can send HTTP request to start / stop recording.
The URL should always be `http://your_host:8000/<selector>`.

Take some examples:

* Recording all ROS topic with the default ROS Domain ID 0

```bash
$ curl -X GET 'http://localhost:8000/@mcap/writer/start'
[
  {
    "key": "@mcap/writer/start",
    "value": {
      "result": "success"
    },
    "encoding": "text/json",
    "timestamp": null
  }
]
```

* Recording the ROS topic with ROS Domain ID 2

```bash
$ curl -X GET 'http://localhost:8000/@mcap/writer/start?domain=2&topic=chatter'
[
  {
    "key": "@mcap/writer/start",
    "value": {
      "result": "success"
    },
    "encoding": "text/json",
    "timestamp": null
  }
]
```

* Get the status of the recorder

```bash
$ curl -X GET 'http://localhost:8000/@mcap/writer/status'
[
  {
    "key": "@mcap/writer/status",
    "value": {
      "status": "running"
    },
    "encoding": "text/json",
    "timestamp": null
  }
]
```

* Stop recording

```bash
$ curl -X GET 'http://localhost:8000/@mcap/writer/stop'
[
  {
    "key": "@mcap/writer/stop",
    "value": {
      "filename": "rosbag2_2025_10_03-17_09_53",
      "result": "success"
    },
    "encoding": "text/json",
    "timestamp": null
  }
]
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

TODO
