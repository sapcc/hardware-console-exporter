FROM keppel.eu-de-1.cloud.sap/ccloud-dockerhub-mirror/library/rust:1.67.0 as build
ENV PKG_CONFIG_ALLOW_CROSS=1

WORKDIR /usr/src/hardware-console-exporter
COPY . .

RUN cargo install --path .

FROM keppel.eu-de-1.cloud.sap/ccloud-gcr-mirror/distroless/cc-debian10
LABEL maintainer="Stefan Hipfel <stefan.hipfel@sap.com>"
LABEL source_repository="https://github.com/sapcc/hardware-console-exporter"

COPY --from=build /usr/local/cargo/bin/hardware-console-exporter /usr/local/bin/hardware-console-exporter

CMD ["hardware-console-exporter"]