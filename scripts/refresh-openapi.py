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
  --refresh   (default) Overwrite all three artifacts from the live source, and
              write a pull-request body summarising how the manifest changed
              against the copy being replaced.
  --check     Download into memory and fail if the live SHA differs from the
              vendored one. This is the human-facing gate: drift exits 1.
  --status    Print `changed=true` or `changed=false` on stdout and exit 0
              either way. The weekly CI workflow branches on this, because
              `make` reports its own exit status 2 for every recipe failure
              and cannot pass a drift exit code through to the caller.

Environment overrides, all optional:
  OPENARCHIEVEN_SPEC_URL     source URL, honoured by every mode.
  OPENARCHIEVEN_OPENAPI_DIR  directory holding the three vendored artifacts.
  OPENARCHIEVEN_BODY_PATH    where --refresh writes the pull-request body.

The last two exist so the tests can drive a real refresh without overwriting the
vendored copy.
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
OPENAPI_DIR = Path(
    os.environ.get("OPENARCHIEVEN_OPENAPI_DIR", REPO_ROOT / "openapi")
).resolve()
SPEC_PATH = OPENAPI_DIR / "openarchieven.yaml"
SHA_PATH = OPENAPI_DIR / "openarchieven.sha256"
MANIFEST_PATH = OPENAPI_DIR / "params-manifest.json"
BODY_PATH = Path(
    os.environ.get(
        "OPENARCHIEVEN_BODY_PATH", REPO_ROOT / "target" / "openapi-refresh-body.md"
    )
).resolve()

HTTP_METHODS = {"get", "post", "put", "patch", "delete", "options", "head"}


def display(path: Path) -> str:
    """Path relative to the repo root when it sits inside it, else absolute.

    The artifact directory is overridable, so a plain `relative_to` raises for
    any path outside the checkout.
    """
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


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


def load_manifest(path: Path) -> dict | None:
    """The committed manifest, or None when there is nothing to compare against.

    A malformed manifest raises instead of reading as "no baseline". Those are
    different facts, and collapsing them would describe a comparison that never
    happened.
    """
    if not path.exists():
        return None
    return json.loads(path.read_text())


def diff_manifests(old: dict, new: dict) -> dict:
    """Structural delta between two manifests, keyed by what a reviewer decides.

    Endpoint sets and query-parameter names are all the manifest holds, so those
    are all this can report; `render_body` says as much in the output.
    """
    old_paths, new_paths = set(old), set(new)
    changed = []
    for path in sorted(old_paths & new_paths):
        before, after = old[path], new[path]
        entry: dict = {"path": path}
        before_params = set(before.get("query_params") or [])
        after_params = set(after.get("query_params") or [])
        if added := sorted(after_params - before_params):
            entry["params_added"] = added
        if removed := sorted(before_params - after_params):
            entry["params_removed"] = removed
        for field in ("method", "operationId"):
            if before.get(field) != after.get(field):
                entry[field] = (before.get(field), after.get(field))
        if len(entry) > 1:
            changed.append(entry)
    return {
        "added": sorted(new_paths - old_paths),
        "removed": sorted(old_paths - new_paths),
        "changed": changed,
    }


def _params(entry: dict) -> str:
    names = entry.get("query_params") or []
    return ", ".join(f"`{name}`" for name in names) if names else "none"


def run_url() -> str | None:
    """Link to the workflow run, when running inside one."""
    server = os.environ.get("GITHUB_SERVER_URL")
    repo = os.environ.get("GITHUB_REPOSITORY")
    run_id = os.environ.get("GITHUB_RUN_ID")
    if server and repo and run_id:
        return f"{server}/{repo}/actions/runs/{run_id}"
    return None


def render_body(delta: dict, new: dict, old: dict | None) -> str:
    """The pull-request body: what changed, then what this cannot tell you.

    Each `###` section holds nothing but its list of changes, so the sections
    stay machine-checkable; every caveat lives under its own heading.
    """
    lines = [
        f"Automated refresh of the vendored OpenAPI spec from `{SPEC_URL}`.",
        "",
        "Updates `openapi/openarchieven.yaml`, `openapi/openarchieven.sha256` and",
        "`openapi/params-manifest.json`.",
        "",
        "## Manifest changes",
        "",
    ]

    if old is None:
        lines += [
            "There is no previous manifest to compare against, so nothing here is",
            "described as added or removed. Read the full spec diff.",
            "",
        ]
    elif not (delta["added"] or delta["removed"] or delta["changed"]):
        lines += [
            "The derived manifest is unchanged: no endpoint, method or query-parameter",
            "name was added or removed. Whatever moved in the spec is confined to the",
            "parts the manifest does not track.",
            "",
        ]
    else:
        if delta["added"]:
            lines += ["### New endpoints", ""]
            lines += [
                f"- `{path}` (`{new[path].get('method')}` "
                f"`{new[path].get('operationId')}`) query params: {_params(new[path])}"
                for path in delta["added"]
            ]
            lines += [""]
        if delta["removed"]:
            lines += ["### Removed endpoints", ""]
            lines += [
                f"- `{path}` (was `{old[path].get('method')}` "
                f"`{old[path].get('operationId')}`)"
                for path in delta["removed"]
            ]
            lines += [""]
        if delta["changed"]:
            lines += ["### Changed endpoints", ""]
            for entry in delta["changed"]:
                lines.append(f"- `{entry['path']}`")
                if "params_added" in entry:
                    listed = ", ".join(f"`{p}`" for p in entry["params_added"])
                    lines.append(f"  - added query params: {listed}")
                if "params_removed" in entry:
                    listed = ", ".join(f"`{p}`" for p in entry["params_removed"])
                    lines.append(f"  - removed query params: {listed}")
                for field in ("method", "operationId"):
                    if field in entry:
                        was, now = entry[field]
                        lines.append(f"  - {field}: `{was}` -> `{now}`")
            lines += [""]

    checks = []
    if delta["added"]:
        checks += [
            "- New endpoints are wrapped by no command yet, so `tests/openapi_contract.rs`",
            "  fails until each one is wrapped or listed in `INTENTIONALLY_UNWRAPPED`.",
        ]
    if any("params_added" in entry for entry in delta["changed"]):
        checks += [
            "- A newly added optional parameter on an existing endpoint breaks nothing, and",
            "  no test asks for it. It is a feature to wrap, and this summary is the only",
            "  place it surfaces.",
        ]
    # Always last, and always present: it is the one caveat that holds no matter
    # what the diff looks like.
    checks += [
        "- The manifest records paths, methods and query-parameter names, nothing else.",
        "  A changed response body, parameter type, or required flag does not appear",
        "  above and no contract test can see it. Read the spec diff for those.",
    ]

    lines += [
        "## What to check",
        "",
        *checks,
        "",
        "## Verification",
        "",
        "`make check` runs against the refreshed spec before this pull request is",
        "opened, so a failing contract test would have stopped it from existing.",
    ]
    if url := run_url():
        lines += ["", f"Run: {url}"]
    lines += [
        "",
        "A pull request opened by `GITHUB_TOKEN` does not trigger workflow runs, so",
        "this one carries no checks of its own.",
        "",
    ]
    return "\n".join(lines)


def write_artifacts(spec_bytes: bytes) -> None:
    previous = load_manifest(MANIFEST_PATH)
    digest = hashlib.sha256(spec_bytes).hexdigest()
    OPENAPI_DIR.mkdir(parents=True, exist_ok=True)
    SPEC_PATH.write_bytes(spec_bytes)
    SHA_PATH.write_text(digest + "\n")
    manifest = build_manifest(spec_bytes)
    MANIFEST_PATH.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    delta = diff_manifests(previous or {}, manifest)
    BODY_PATH.parent.mkdir(parents=True, exist_ok=True)
    BODY_PATH.write_text(render_body(delta, manifest, previous))

    print(f"wrote {display(SPEC_PATH)} ({len(spec_bytes)} bytes)")
    print(f"wrote {display(SHA_PATH)} ({digest})")
    print(f"wrote {display(MANIFEST_PATH)} ({len(manifest)} paths)")
    print(
        f"wrote {display(BODY_PATH)} "
        f"({len(delta['added'])} added, {len(delta['removed'])} removed, "
        f"{len(delta['changed'])} changed)"
    )


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
        f"no vendored sha at {display(SHA_PATH)}; run --refresh first",
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
