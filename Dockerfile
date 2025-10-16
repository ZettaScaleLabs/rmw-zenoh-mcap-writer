FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Install basic tools and prerequisites
RUN apt-get update && \
    apt-get install -y \
        curl gpg ca-certificates wget \
        git build-essential cmake pkg-config \
        python3 python3-pip \
        just \
        && rm -rf /var/lib/apt/lists/*

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Copy the code
WORKDIR /workspace
COPY . /workspace

# Build
RUN cargo build --release
