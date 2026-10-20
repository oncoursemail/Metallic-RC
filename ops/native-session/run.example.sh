#!/usr/bin/env bash
# Foreground private-lab example. Does not install or change any system service.
set -euo pipefail

: "${METALLIC_RC_CHECKOUT:?Set the absolute Metallic-RC checkout path}"
: "${METALLIC_RC_BUILD_MANIFEST:?Set the absolute reviewed server BUILD.json path}"
: "${METALLIC_RC_DATA_DIR:?Set the absolute owner-private session directory}"
: "${METALLIC_RC_MANIFEST_SHA256:?Set the separately reviewed BUILD.json SHA-256}"

exec python3 "${METALLIC_RC_CHECKOUT}/services/session-runner/session_runner.py" run \
  --manifest "${METALLIC_RC_BUILD_MANIFEST}" \
  --data-dir "${METALLIC_RC_DATA_DIR}" \
  --port "${METALLIC_RC_SESSION_PORT:-27841}" \
  --expected-manifest-sha256 "${METALLIC_RC_MANIFEST_SHA256}" \
  --max-attempts 3 \
  --ready-timeout 10 \
  --stop-timeout 3 \
  --backoff 0.5 \
  --backoff-cap 8
