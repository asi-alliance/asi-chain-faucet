FROM rust:slim

WORKDIR faucet

COPY . .

RUN ["apt", "update"]
RUN ["apt", "upgrade", "--yes"]
RUN ["apt", "install", "--yes", "make", "perl", "protobuf-compiler"]

RUN ["cargo", "build"]

CMD ["cargo", "run"]
