FROM rust:1.67.0 as build
ENV PKG_CONFIG_ALLOW_CROSS=1

WORKDIR /usr/src/hardware-console-exporter
COPY . .

RUN cargo install --path .

FROM gcr.io/distroless/cc-debian10

COPY --from=build /usr/local/cargo/bin/hardware-console-exporter /usr/local/bin/hardware-console-exporter

CMD ["hardware-console-exporter"]