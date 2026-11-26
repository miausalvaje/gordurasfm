#!/bin/sh
# Copia el código al LXC de desarrollo (sin artefactos de compilación).
rsync -az --delete --exclude target --exclude 'web/build' --exclude 'web/.dart_tool' --exclude '.flutter-plugins*' \
  --exclude '.git' --exclude 'data' --exclude 'dist' "$(dirname "$0")/" gfm-dev:GordurasFM/
