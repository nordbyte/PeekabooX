FROM rust:1-trixie AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        libdbus-1-dev \
        pkg-config \
        python3 \
        python3-pip \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

RUN cargo build --release -p peekaboox-cli -p peekabooxd
RUN python3 -m pip wheel --no-deps -w /tmp/peekaboox-wheel ./python

FROM debian:trixie-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libdbus-1-3 \
        python3 \
        python3-pip \
        tesseract-ocr \
        xdg-desktop-portal \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/peekaboox /usr/local/bin/peekaboox
COPY --from=builder /src/target/release/peekabooxd /usr/local/bin/peekabooxd
COPY --from=builder /tmp/peekaboox-wheel /opt/peekaboox/wheels
COPY examples /usr/share/peekaboox/examples
COPY docs /usr/share/doc/peekaboox
COPY README.md CHANGELOG.md /usr/share/doc/peekaboox/

RUN python3 -m pip install --break-system-packages /opt/peekaboox/wheels/peekaboox-*.whl

ENTRYPOINT ["peekaboox"]
CMD ["--version"]

FROM runtime AS smoke
RUN peekaboox --version \
    && peekabooxd --version \
    && peekaboox plugins --path /usr/share/peekaboox/examples/plugins \
    && peekaboox-mcp --list-tools
