#!/bin/sh
# Roda scripts/validar.sh num contêiner (sem Rust local). A imagem e os caches ficam no Docker.
set -e
docker build -q -t baseconhecimento-validar -f docker/validar.Dockerfile docker >/dev/null
docker run --rm \
  -v baseconhecimento-cargo:/usr/local/cargo/registry \
  -v baseconhecimento-target:/w/target \
  -v "$(pwd -W 2>/dev/null || pwd)":/w -w /w \
  baseconhecimento-validar sh scripts/validar.sh "$@"
