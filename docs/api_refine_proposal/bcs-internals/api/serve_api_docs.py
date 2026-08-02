#!/usr/bin/env python3
"""Serve BCN API docs through Swagger UI.

The API specs in this directory are split by business area. Swagger UI is much
easier to use when each audience sees one bundled spec, so this script merges
the public OpenAPI files and internal API files at request time.
"""

from __future__ import annotations

import argparse
import copy
import json
import sys
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

try:
    import yaml
except ModuleNotFoundError as exc:  # pragma: no cover - startup guard
    raise SystemExit(
        "PyYAML is required. Try: python3 -m pip install pyyaml"
    ) from exc


API_DIR = Path(__file__).resolve().parent
METHODS = {"get", "put", "post", "delete", "patch", "options", "head", "trace"}


def load_yaml(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as file:
        data = yaml.safe_load(file)
    if not isinstance(data, dict):
        raise ValueError(f"{path} does not contain a YAML object")
    return data


def rewrite_refs(value: Any) -> Any:
    """Rewrite split-file refs so they point at the bundled root document."""

    if isinstance(value, dict):
        result: dict[str, Any] = {}
        for key, item in value.items():
            if key == "$ref" and isinstance(item, str):
                if item.startswith("../_shared.yaml#"):
                    result[key] = item.replace("../_shared.yaml#", "#", 1)
                else:
                    result[key] = item
            else:
                result[key] = rewrite_refs(item)
        return result
    if isinstance(value, list):
        return [rewrite_refs(item) for item in value]
    return value


def merge_components(
    target: dict[str, Any], source: dict[str, Any], origin: Path
) -> None:
    for section, entries in (source.get("components") or {}).items():
        if not isinstance(entries, dict):
            continue
        target_section = target.setdefault(section, {})
        for name, value in entries.items():
            merged_value = rewrite_refs(copy.deepcopy(value))
            if merged_value == {"$ref": f"#/components/{section}/{name}"}:
                continue
            if name in target_section and target_section[name] != merged_value:
                raise ValueError(
                    f"component conflict: components/{section}/{name} from {origin}"
                )
            target_section[name] = merged_value


def merge_tags(target: list[dict[str, Any]], source: list[Any], origin: Path) -> None:
    by_name = {
        tag.get("name"): tag
        for tag in target
        if isinstance(tag, dict) and isinstance(tag.get("name"), str)
    }
    for tag in source:
        if not isinstance(tag, dict) or not isinstance(tag.get("name"), str):
            continue
        existing = by_name.get(tag["name"])
        if existing is not None and existing != tag:
            raise ValueError(f"tag conflict: {tag['name']} from {origin}")
        if existing is None:
            target.append(rewrite_refs(copy.deepcopy(tag)))
            by_name[tag["name"]] = target[-1]


def merge_paths(
    target: dict[str, Any], source: dict[str, Any], origin: Path
) -> None:
    for route, item in (source.get("paths") or {}).items():
        if route not in target:
            target[route] = rewrite_refs(copy.deepcopy(item))
            continue
        if not isinstance(item, dict) or not isinstance(target[route], dict):
            raise ValueError(f"path conflict: {route} from {origin}")
        for key, value in item.items():
            if key in target[route] and target[route][key] != rewrite_refs(value):
                raise ValueError(f"path operation conflict: {key.upper()} {route}")
            target[route][key] = rewrite_refs(copy.deepcopy(value))


def newest_version(files: list[Path], fallback: str) -> str:
    versions: list[str] = []
    for file in files:
        version = load_yaml(file).get("info", {}).get("version")
        if version is not None:
            versions.append(str(version))
    return max(versions) if versions else fallback


def build_bundle(kind: str) -> dict[str, Any]:
    if kind not in {"openapi", "internalapi"}:
        raise ValueError(f"unknown spec kind: {kind}")

    shared = load_yaml(API_DIR / "_shared.yaml")
    files = sorted((API_DIR / kind).glob("*.yaml"))
    if not files:
        raise ValueError(f"no YAML files found for {kind}")

    first = load_yaml(files[0])
    title = "BCN OpenAPI" if kind == "openapi" else "BCN Internal API"
    description = (
        "Bundled BCN public OpenAPI generated from src/bcs/docs/api/openapi/*.yaml."
        if kind == "openapi"
        else "Bundled BCN internal API generated from src/bcs/docs/api/internalapi/*.yaml."
    )

    bundle: dict[str, Any] = {
        "openapi": first.get("openapi", "3.1.0"),
        "info": {
            "title": title,
            "version": newest_version(files, first.get("info", {}).get("version", "2026-07-06")),
            "description": description,
        },
        "servers": first.get("servers", []),
        "tags": [],
        "paths": {},
        "components": {},
    }
    if first.get("security"):
        bundle["security"] = rewrite_refs(copy.deepcopy(first["security"]))

    merge_components(bundle["components"], shared, API_DIR / "_shared.yaml")
    for file in files:
        spec = load_yaml(file)
        merge_tags(bundle["tags"], spec.get("tags") or [], file)
        merge_paths(bundle["paths"], spec, file)
        merge_components(bundle["components"], spec, file)

    assert_no_external_refs(bundle)
    return bundle


def assert_no_external_refs(value: Any) -> None:
    stack = [value]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            ref = item.get("$ref")
            if isinstance(ref, str) and not ref.startswith("#/"):
                raise ValueError(f"unresolved external $ref after bundling: {ref}")
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)


def count_operations(spec: dict[str, Any]) -> int:
    total = 0
    for item in (spec.get("paths") or {}).values():
        if not isinstance(item, dict):
            continue
        total += sum(1 for method in item if method.lower() in METHODS)
    return total


def render_yaml(spec: dict[str, Any]) -> bytes:
    return yaml.safe_dump(spec, allow_unicode=True, sort_keys=False).encode("utf-8")


def render_html() -> bytes:
    html = """<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>BCN API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  <style>
    body { margin: 0; background: #f7f8fa; }
    #header {
      padding: 16px 24px;
      background: #111827;
      color: #fff;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 24px;
      flex-wrap: wrap;
    }
    #header h1 { margin: 0 0 4px; font-size: 20px; font-weight: 650; }
    #header p { margin: 0; color: #cbd5e1; font-size: 13px; }
    #api-switcher {
      display: flex;
      align-items: center;
      gap: 10px;
      color: #cbd5e1;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      font-size: 13px;
      white-space: nowrap;
    }
    #api-spec-selector {
      min-width: 210px;
      border: 1px solid #475569;
      border-radius: 6px;
      background: #fff;
      color: #111827;
      font-size: 14px;
      padding: 7px 10px;
    }
  </style>
</head>
<body>
  <div id="header">
    <div>
      <h1>BCN API Docs</h1>
      <p>Use the API document selector to switch between public OpenAPI and internal API.</p>
    </div>
    <label id="api-switcher" for="api-spec-selector">
      API 文档
      <select id="api-spec-selector">
        <option value="openapi">BCN OpenAPI</option>
        <option value="internalapi">BCN Internal API</option>
      </select>
    </label>
  </div>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
  <script>
    window.onload = () => {
      const specs = {
        openapi: { url: '/openapi.yaml', name: 'BCN OpenAPI' },
        internalapi: { url: '/internalapi.yaml', name: 'BCN Internal API' },
      };
      const params = new URLSearchParams(window.location.search);
      const requestedSpec = params.get('spec');
      const selectedKey = specs[requestedSpec] ? requestedSpec : 'openapi';
      const selector = document.getElementById('api-spec-selector');
      selector.value = selectedKey;
      selector.addEventListener('change', (event) => {
        const next = event.target.value;
        const nextUrl = new URL(window.location.href);
        nextUrl.searchParams.set('spec', next);
        window.location.href = nextUrl.toString();
      });
      document.title = `${specs[selectedKey].name} - BCN API Docs`;
      SwaggerUIBundle({
        dom_id: '#swagger-ui',
        url: specs[selectedKey].url,
        deepLinking: true,
        presets: [
          SwaggerUIBundle.presets.apis,
          SwaggerUIStandalonePreset
        ],
        layout: 'StandaloneLayout',
        tryItOutEnabled: false,
      });
    };
  </script>
</body>
</html>
"""
    return html.encode("utf-8")


class ApiDocsHandler(BaseHTTPRequestHandler):
    server_version = "BcnApiDocs/1.0"

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        specs = self.server.specs  # type: ignore[attr-defined]
        if self.path in {"/", "/index.html"}:
            self.respond(200, "text/html; charset=utf-8", render_html())
        elif self.path == "/openapi.yaml":
            self.respond(200, "application/yaml; charset=utf-8", render_yaml(specs["openapi"]))
        elif self.path == "/internalapi.yaml":
            self.respond(200, "application/yaml; charset=utf-8", render_yaml(specs["internalapi"]))
        elif self.path == "/openapi.json":
            self.respond_json(200, specs["openapi"])
        elif self.path == "/internalapi.json":
            self.respond_json(200, specs["internalapi"])
        elif self.path == "/healthz":
            self.respond_json(200, {"ok": True})
        elif self.path == "/favicon.ico":
            self.respond(204, "image/x-icon", b"")
        else:
            self.respond_json(404, {"error": "not found"})

    def log_message(self, fmt: str, *args: Any) -> None:
        sys.stderr.write("%s - %s\n" % (self.log_date_time_string(), fmt % args))

    def respond(self, status: int, content_type: str, body: bytes) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        if body:
            self.wfile.write(body)

    def respond_json(self, status: int, payload: Any) -> None:
        self.respond(
            status,
            "application/json; charset=utf-8",
            json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8"),
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Serve BCN API docs with Swagger UI.")
    parser.add_argument("--host", default="127.0.0.1", help="bind host")
    parser.add_argument("--port", type=int, default=8765, help="bind port")
    parser.add_argument("--check", action="store_true", help="build bundled specs and exit")
    parser.add_argument("--open", action="store_true", help="open the docs page in a browser")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    specs = {
        "openapi": build_bundle("openapi"),
        "internalapi": build_bundle("internalapi"),
    }

    if args.check:
        for name, spec in specs.items():
            print(
                f"{name}: paths={len(spec.get('paths') or {})} "
                f"operations={count_operations(spec)}"
            )
        return 0

    server = ThreadingHTTPServer((args.host, args.port), ApiDocsHandler)
    server.specs = specs  # type: ignore[attr-defined]
    host, port = server.server_address
    url = f"http://{host}:{port}/"
    print(f"BCN API docs server listening on {url}")
    print("Specs: /openapi.yaml, /internalapi.yaml, /openapi.json, /internalapi.json")
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping BCN API docs server.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
