FROM rust:1.77
WORKDIR /usr/src/app
COPY . .
RUN git clone https://github.com/F1R3FLY-io/rust-client.git && \
    cd rust-client && \
    cargo build --release && \
    cp target/release/node_cli /usr/local/bin/ && \
    cd .. && \
    rm -rf rust-client
    
ENV SERVER_PORT=40470
EXPOSE $SERVER_PORT
RUN cargo run
