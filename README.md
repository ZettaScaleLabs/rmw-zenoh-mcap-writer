# rmw-zenoh-mcap-writer

The MCAP writer is a standalone tool able to subscribe to all or a subset of ROS 2 topics published via `rmw_zenoh` and write the received messages into a MCAP file with a rosbag-compatible format.

It's developped directly on top of Zenoh (in Rust). Hence it doesn't require ROS to be installed on the host it runs. It can directly subscribe to the ROS 2 publications remotely connecting to the Zenoh router running on ROS host.

The subscriptions and recording is controlled via a Zenoh-based API.

## API

When `rmw-zenoh-mcap-writer` is running, it declares a Queryable on the key expression `@mcap/writer/*` to receive commands.
Any application using Zenoh can send commands as a Zenoh Query with a matching key expression.

The supported command are:

### Start a recording

* **Key expression**: `@mcap/writer/start`

* **Optional Query parameters**:
  * `domain`: The ROS domain ID. It's `0` if not sepecified.
  * `topic`: The list of ROS topic to record, using `,` as separator. If not specified, all topics are recorded.  
    The Zenoh wildcards characters are supported: `*` matching 1 chunk (string between `/`) and `**` matching several chunks. For instance with a depth camera:  
    **`/bot1/camera/*`** is matching `/bot1/camera/camera_info`, `/bot1/camera/image_raw` and `/bot1/camera/points`.  
    **`/bot1/camera/**`** is matching the same, plus `/bot1/camera/depth/camera_info` and `/bot1/camera/depth/image_raw`.

* **Return value**: a JSON string

  ```json
  {
    "result": "success",  // It can be either "success" or "failure"
  }
  ```

<details>
  <summary>Example of usage with REST API</summary>

Assuming `rmw-zenoh-mcap-writer` runs the REST API on port `8000` - which is the default - you can use the `curl` command.

* Recording all ROS topics with the default ROS Domain ID 0

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

* Recording some specific ROS topics with ROS Domain ID 2

  ```bash
  $ curl -X GET 'http://localhost:8000/@mcap/writer/start?domain=2;topic=/camera/*,chatter'
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

</details>

<details>
  <summary>Example of usage in Python</summary>

* Recording all ROS topics with the default ROS Domain ID 0

  ```python
  replies = list(session.get('@mcap/writer/start'))
  if len(replies) == 0:
    print("No reply. Is rmw-zenoh-mcap-writer running ?")
  elif not replies[0].ok or not replies[0].ok.payload:
    print("Error in reply")
  else:
    print(f"Reply: '{replies[0].ok.payload.to_string()}'")
  ```

* Recording some specific ROS topics with ROS Domain ID 2

  ```python
  replies = list(session.get('@mcap/writer/start?domain=2;topic=/camera/*,chatter'))
  # ... same as above
  ```

</details>

### Stop a recording

* **Key expression**: `@mcap/writer/stop`

* **Optional Query parameters**: none

* **Return value**: a JSON string

  ```json
  {
    "result": "success",  // It can be either "success" or "failure"
    "filename": "rosbag2_2025_10_03-17_09_53.mcap", // The stored rosbag filename
  }
  ```

<details>
  <summary>Example of usage with REST API</summary>

Assuming `rmw-zenoh-mcap-writer` runs the REST API on port `8000` - which is the default - you can use the `curl` command.

```bash
$ curl -X GET 'http://localhost:8000/@mcap/writer/stop'
[
  {
    "key": "@mcap/writer/stop",
    "value": {
      "filename": "rosbag2_2025_10_03-17_09_53.mcap",
      "result": "success"
    },
    "encoding": "text/json",
    "timestamp": null
  }
]
```

</details>

<details>
  <summary>Example of usage in Python</summary>

```python
replies = list(session.get('@mcap/writer/stop'))
if len(replies) == 0:
  print("No reply. Is rmw-zenoh-mcap-writer running ?")
elif not replies[0].ok or not replies[0].ok.payload:
  print("Error in reply")
else:
  print(f"Reply: '{replies[0].ok.payload.to_string()}'")
```

</details>

### Get the status of the recorder

* **Key expression**: `@mcap/writer/status`

* **Optional Query parameters**: none

* **Return value**: a JSON string

  ```json
  {
    "status": "recording",  // It can be either "recording" or "stopped"
  }
  ```

<details>
  <summary>Example of usage with REST API</summary>

Assuming `rmw-zenoh-mcap-writer` runs the REST API on port `8000` - which is the default - you can use the `curl` command.

```bash
$ curl -X GET 'http://localhost:8000/@mcap/writer/status'
[
  {
    "key": "@mcap/writer/status",
    "value": {
      "status": "recording"
    },
    "encoding": "text/json",
    "timestamp": null
  }
]
```

</details>

<details>
  <summary>Example of usage in Python</summary>

```python
replies = list(session.get('@mcap/writer/status'))
if len(replies) == 0:
  print("No reply. Is rmw-zenoh-mcap-writer running ?")
elif not replies[0].ok or not replies[0].ok.payload:
  print("Error in reply")
else:
  print(f"Reply: '{replies[0].ok.payload.to_string()}'")
```

</details>

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
