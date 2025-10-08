FROM python:3.11-slim AS builder

#WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y \
    curl \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

#ENV PATH="/root/.cargo/bin:${PATH}"
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"
#RUN git clone https://github.com/F1R3FLY-io/rust-client.git

COPY . .

#RUN cargo run

#FROM python:3.11-slim

#WORKDIR /usr/src/app

#RUN apt-get update && apt-get install -y \
#    libpq5 \
#    libssl-dev \
#    && rm -rf /var/lib/apt/lists/*

#COPY --from=builder /usr/src/app/target/release/asi-faucet /usr/local/bin/asi-faucet
#COPY --from=builder /usr/src/app/rust-client/target/release/node_cli /usr/local/bin/node_cli

ENV SERVER_PORT=40470
EXPOSE $SERVER_PORT

#CMD ["asi-faucet"]
RUN cargo run
