#!/usr/bin/env -S uv run --quiet --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pyyaml>=6"]
# ///
"""Refresh the vendored OpenAPI spec and derived manifest.

Downloads `https://api.openarchieven.nl/openapi.yaml`, writes it to
`openapi/openarchieven.yaml`, writes its sha256 to `openapi/openarchieven.sha256`,
and emits a minimal `openapi/params-manifest.json` keyed by upstream path. Each
entry lists every accepted query-parameter name; the Rust contract test asserts
that outbound CLI requests use only names that appear in the matching entry.

Modes:
  --refresh   (default) Overwrite all three artifacts from the live source.
  --check     Download into memory and fail if the live SHA differs from the
              vendored one. This is the human-facing gate: drift exits 1.
  --status    Print `changed=true` or `changed=false` on stdout and exit 0
              either way. The weekly CI workflow branches on this, because
              `make` reports its own exit status 2 for every recipe failure
              and cannot pass a drift exit code through to the caller.

`OPENARCHIEVEN_SPEC_URL` overrides the source URL for every mode.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import urllib.request
from pathlib import Path

import yaml

SPEC_URL = os.environ.get(
    "OPENARCHIEVEN_SPEC_URL", "https://api.openarchieven.nl/openapi.yaml"
)
USER_AGENT = "openarchieven-cli/refresh-openapi"

REPO_ROOT = Path(__file__).resolve().parent.parent
SPEC_PATH = REPO_ROOT / "openapi" / "openarchieven.yaml"
SHA_PATH = REPO_ROOT / "openapi" / "openarchieven.sha256"
MANIFEST_PATH = REPO_ROOT / "openapi" / "params-manifest.json"

HTTP_METHODS = {"get", "post", "put", "patch", "delete", "options", "head"}


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=30) as resp:
        return resp.read()


def build_manifest(spec_bytes: bytes) -> dict:
    doc = yaml.safe_load(spec_bytes)
    paths = doc.get("paths") or {}
    manifest = {}
    for path, ops in sorted(paths.items()):
        if not isinstance(ops, dict):
            continue
        for method, op in ops.items():
            if method.lower() not in HTTP_METHODS or not isinstance(op, dict):
                continue
            params = op.get("parameters") or []
            query_params = sorted(
                p["name"]
                for p in params
                if isinstance(p, dict) and p.get("in") == "query" and "name" in p
            )
            manifest[path] = {
                "operationId": op.get("operationId") or f"{method.upper()} {path}",
                "method": method.upper(),
                "query_params": query_params,
            }
            # We only wrap GETs; first one wins, but the API is read-only.
            break
    return manifest


def write_artifacts(spec_bytes: bytes) -> None:
    digest = hashlib.sha256(spec_bytes).hexdigest()
    SPEC_PATH.write_bytes(spec_bytes)
    SHA_PATH.write_text(digest + "\n")
    manifest = build_manifest(spec_bytes)
    MANIFEST_PATH.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(f"wrote {SPEC_PATH.relative_to(REPO_ROOT)} ({len(spec_bytes)} bytes)")
    print(f"wrote {SHA_PATH.relative_to(REPO_ROOT)} ({digest})")
    print(f"wrote {MANIFEST_PATH.relative_to(REPO_ROOT)} ({len(manifest)} paths)")


def compare_with_live() -> tuple[bool, str, str]:
    """Return (drifted, vendored_digest, live_digest).

    Raises FileNotFoundError when there is nothing vendored to compare against;
    a missing baseline is neither drift nor agreement and must not be reported
    as either. Fetch failures propagate for the same reason.
    """
    live_digest = hashlib.sha256(fetch(SPEC_URL)).hexdigest()
    if not SHA_PATH.exists():
        raise FileNotFoundError(SHA_PATH)
    vendored_digest = SHA_PATH.read_text().strip()
    return live_digest != vendored_digest, vendored_digest, live_digest


def missing_baseline_message() -> None:
    print(
        f"no vendored sha at {SHA_PATH.relative_to(REPO_ROOT)}; run --refresh first",
        file=sys.stderr,
    )


def check_drift() -> int:
    try:
        drifted, vendored_digest, live_digest = compare_with_live()
    except FileNotFoundError:
        missing_baseline_message()
        return 2
    if not drifted:
        print(f"openapi spec up to date (sha256 {vendored_digest[:12]}…)")
        return 0
    print(
        "upstream openapi spec has drifted",
        f"  vendored: {vendored_digest}",
        f"  live:     {live_digest}",
        "run `make openapi-refresh` and commit the result",
        sep="\n",
        file=sys.stderr,
    )
    return 1


def report_status() -> int:
    """Emit the drift verdict as data on stdout, keeping exit codes for errors.

    Callers consume stdout verbatim (the CI workflow appends it to
    `$GITHUB_OUTPUT`), so the verdict is the only thing written there and every
    diagnostic goes to stderr.
    """
    try:
        drifted, vendored_digest, live_digest = compare_with_live()
    except FileNotFoundError:
        missing_baseline_message()
        return 2
    print("changed=true" if drifted else "changed=false")
    print(
        f"vendored: {vendored_digest}",
        f"live:     {live_digest}",
        sep="\n",
        file=sys.stderr,
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--refresh", action="store_true", help="download and overwrite (default)")
    mode.add_argument("--check", action="store_true", help="fail if vendored sha differs from live")
    mode.add_argument(
        "--status",
        action="store_true",
        help="print changed=true|false on stdout and exit 0",
    )
    args = parser.parse_args()

    if args.status:
        return report_status()
    if args.check:
        return check_drift()

    spec_bytes = fetch(SPEC_URL)
    write_artifacts(spec_bytes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
