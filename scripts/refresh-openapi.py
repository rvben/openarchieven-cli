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
              vendored one. CI uses this on a schedule to detect upstream drift.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import urllib.request
from pathlib import Path

import yaml

SPEC_URL = "https://api.openarchieven.nl/openapi.yaml"
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


def check_drift() -> int:
    live = fetch(SPEC_URL)
    live_digest = hashlib.sha256(live).hexdigest()
    if not SHA_PATH.exists():
        print(
            f"no vendored sha at {SHA_PATH.relative_to(REPO_ROOT)}; run --refresh first",
            file=sys.stderr,
        )
        return 2
    vendored_digest = SHA_PATH.read_text().strip()
    if live_digest == vendored_digest:
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--refresh", action="store_true", help="download and overwrite (default)")
    mode.add_argument("--check", action="store_true", help="fail if vendored sha differs from live")
    args = parser.parse_args()

    if args.check:
        return check_drift()

    spec_bytes = fetch(SPEC_URL)
    write_artifacts(spec_bytes)
    return 0


if __name__ == "__main__":
    sys.exit(main())
