# syntax=docker/dockerfile:1
# Imagen mínima de GordurasFM: el binario estático (musl) y nada más (FROM scratch).
# El binario trae la web, los certificados raíz, las zonas horarias y su propio resolutor DNS.
#
# Antes de construir: scripts/build-release.sh (deja dist/linux/{amd64,arm64}/gordurasfm).
#   docker buildx build --platform linux/amd64,linux/arm64 -t gordurasfm .

FROM --platform=$BUILDPLATFORM busybox:1.37 AS data
RUN mkdir /data && chown 65532:65532 /data

FROM scratch
ARG TARGETPLATFORM
COPY --from=data --chown=65532:65532 /data /data
COPY --chmod=755 dist/${TARGETPLATFORM}/gordurasfm /gordurasfm
USER 65532:65532
ENV GFM_DATA_DIR=/data \
    GFM_LISTEN=0.0.0.0:8090
VOLUME /data
EXPOSE 8090
HEALTHCHECK --interval=30s --timeout=6s --start-period=20s --retries=3 CMD ["/gordurasfm", "healthcheck"]
ENTRYPOINT ["/gordurasfm"]
CMD ["serve"]

LABEL org.opencontainers.image.title="GordurasFM" \
      org.opencontainers.image.description="Estadísticas de escucha autoalojadas y Wrapped para Navidrome" \
      org.opencontainers.image.source="https://github.com/miausalvaje/gordurasfm" \
      org.opencontainers.image.licenses="AGPL-3.0-only"
