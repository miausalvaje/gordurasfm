#!/bin/sh
# Copia el código al LXC de desarrollo (sin artefactos de compilación).
rsync -az --delete --exclude target --exclude 'web/build' --exclude 'web/.dart_tool' --exclude '.git' --exclude 'data' "$(dirname "$0")/" gfm-dev:GordurasFM/
