#!/usr/bin/env bash
set -e

DOCKER_FILE=${DOCKER_FILE:-Dockerfile}
DOCKER_IMAGE=${DOCKER_IMAGE:-ros2-kilted-dev}
CONTAINER_NAME=${DOCKER_IMAGE}-container

if [ ! "$(docker images -q ${DOCKER_IMAGE})" ]; then
    echo "${DOCKER_IMAGE} does not exist. Creating..."
    docker build -f ${DOCKER_FILE} -t ${DOCKER_IMAGE} .
fi

# Check the status of the container (We need to ignore errors here)
set +e
CONTAINER_STATUS=$(docker inspect --format='{{.State.Status}}' $CONTAINER_NAME 2>/dev/null)
set -e

if [ "$CONTAINER_STATUS" == "running" ]; then
    echo "Container '$CONTAINER_NAME' is already running. Attaching to the container..."
    docker exec -it ${CONTAINER_NAME} bash
else
    echo "Container '$CONTAINER_NAME' does not exist. Creating and running the container..."
    docker run --rm -it --network host --privileged --name ${CONTAINER_NAME} ${DOCKER_IMAGE}
fi
