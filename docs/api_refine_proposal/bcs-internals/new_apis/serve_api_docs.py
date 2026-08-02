#!/usr/bin/env python3
"""Validate, bundle, and serve the BCN target API documents."""

from __future__ import annotations

import argparse
import copy
import json
import sys
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Iterable
from urllib.parse import parse_qs, urlparse

try:
    import yaml
except ModuleNotFoundError as exc:  # pragma: no cover - startup guard
    raise SystemExit("PyYAML is required. Try: python3 -m pip install pyyaml") from exc


API_DIR = Path(__file__).resolve().parent
METHODS = {"get", "put", "post", "delete", "patch", "options", "head", "trace"}
SPEC_KEYS = ("all", "openapi", "internalapi", "domain-models")
ERROR_CODES_EXTENSION = "x-bcn-error-codes"
SUCCESS_MESSAGES = {200: "OK", 201: "Created", 202: "Accepted"}
DOMAIN_MODEL_VIEW_SCHEMAS = (
    "Actor",
    "Group",
    "Session",
    "Provider",
    "ProviderBotBinding",
    "Friendship",
    "FriendRequest",
    "Invitation",
    "CollaborationTemplate",
    "StateMachineRun",
)


def load_yaml(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as file:
        data = yaml.safe_load(file)
    if not isinstance(data, dict):
        raise ValueError(f"{path} does not contain a YAML object")
    return data


def rewrite_refs(value: Any) -> Any:
    """Rewrite refs to shared/model files so a bundled document is standalone."""

    if isinstance(value, dict):
        result: dict[str, Any] = {}
        for key, item in value.items():
            if key == "$ref" and isinstance(item, str):
                prefixes = ("../_shared.yaml#", "../domain-models.yaml#")
                replacement = item
                for prefix in prefixes:
                    if replacement.startswith(prefix):
                        replacement = replacement.replace(prefix, "#", 1)
                        break
                result[key] = replacement
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
        normalized = rewrite_refs(copy.deepcopy(tag))
        existing = by_name.get(tag["name"])
        if existing is not None and existing != normalized:
            raise ValueError(f"tag conflict: {tag['name']} from {origin}")
        if existing is None:
            target.append(normalized)
            by_name[tag["name"]] = normalized


def merge_paths(target: dict[str, Any], source: dict[str, Any], origin: Path) -> None:
    for route, item in (source.get("paths") or {}).items():
        rewritten = rewrite_refs(copy.deepcopy(item))
        if route not in target:
            target[route] = rewritten
            continue
        if not isinstance(rewritten, dict) or not isinstance(target[route], dict):
            raise ValueError(f"path conflict: {route} from {origin}")
        for key, value in rewritten.items():
            if key in target[route] and target[route][key] != value:
                raise ValueError(f"path operation conflict: {key.upper()} {route}")
            target[route][key] = value


def newest_version(files: Iterable[Path], fallback: str = "2026-07-20") -> str:
    versions = [str(load_yaml(path).get("info", {}).get("version", fallback)) for path in files]
    return max(versions) if versions else fallback


def fragment_files(kind: str) -> list[Path]:
    files = sorted((API_DIR / kind).glob("*.yaml"))
    if not files:
        raise ValueError(f"no YAML files found for {kind}")
    return files


def build_api_bundle(kind: str) -> dict[str, Any]:
    if kind not in {"openapi", "internalapi", "all"}:
        raise ValueError(f"unknown API bundle kind: {kind}")

    groups = [kind] if kind != "all" else ["openapi", "internalapi"]
    files = [path for group in groups for path in fragment_files(group)]
    titles = {
        "all": "BCN All APIs",
        "openapi": "BCN OpenAPI",
        "internalapi": "BCN Internal API",
    }
    descriptions = {
        "all": "BCN target OpenAPI and Internal API grouped by common business tags.",
        "openapi": "BCN target public OpenAPI.",
        "internalapi": "BCN target Internal API for trusted callers.",
    }
    bundle: dict[str, Any] = {
        "openapi": "3.1.0",
        "info": {
            "title": titles[kind],
            "version": newest_version(files),
            "description": descriptions[kind],
        },
        "servers": [{"url": "https://bcn-prod.alipay.com", "description": "生产环境"}],
        "tags": [],
        "paths": {},
        "components": {},
    }

    shared_path = API_DIR / "_shared.yaml"
    models_path = API_DIR / "domain-models.yaml"
    merge_components(bundle["components"], load_yaml(shared_path), shared_path)
    merge_components(bundle["components"], load_yaml(models_path), models_path)

    for file in files:
        spec = load_yaml(file)
        merge_tags(bundle["tags"], spec.get("tags") or [], file)
        merge_paths(bundle["paths"], spec, file)
        merge_components(bundle["components"], spec, file)

    materialize_response_contracts(bundle)
    validate_bundle(bundle, kind)
    return bundle


def resolve_response_ref(spec: dict[str, Any], response: Any) -> Any:
    if not isinstance(response, dict):
        return response
    ref = response.get("$ref")
    prefix = "#/components/responses/"
    if not isinstance(ref, str) or not ref.startswith(prefix):
        return copy.deepcopy(response)
    name = ref.removeprefix(prefix)
    responses = (spec.get("components") or {}).get("responses") or {}
    if name not in responses:
        raise ValueError(f"unknown response component: {name}")
    return copy.deepcopy(responses[name])


def response_media_schema(response: dict[str, Any]) -> tuple[dict[str, Any], Any] | None:
    content = response.get("content")
    if not isinstance(content, dict):
        return None
    media = content.get("application/json")
    if not isinstance(media, dict) or "schema" not in media:
        return None
    return media, media["schema"]


def materialize_response_contracts(spec: dict[str, Any]) -> None:
    """Turn concise operation metadata into strict Swagger response contracts."""

    for _, _, operation in iter_operations(spec):
        errors = operation.get(ERROR_CODES_EXTENSION) or {}
        for raw_status, raw_response in list((operation.get("responses") or {}).items()):
            if not str(raw_status).isdigit():
                continue
            status = int(raw_status)
            response = resolve_response_ref(spec, raw_response)
            if not isinstance(response, dict):
                continue
            media_schema = response_media_schema(response)
            if media_schema is None:
                operation["responses"][raw_status] = response
                continue
            media, base_schema = media_schema

            if 200 <= status < 300:
                message = SUCCESS_MESSAGES.get(status, "OK")
                media["schema"] = {
                    "allOf": [
                        copy.deepcopy(base_schema),
                        {
                            "type": "object",
                            "required": ["code", "message"],
                            "properties": {
                                "code": {
                                    "type": "integer",
                                    "const": status * 100,
                                    "description": f"HTTP {status} 的成功业务码。",
                                },
                                "message": {
                                    "type": "string",
                                    "const": message,
                                    "description": "与成功业务码对应的稳定消息。",
                                },
                            },
                        },
                    ]
                }
            elif str(raw_status) in errors:
                variants = []
                examples: dict[str, Any] = {}
                for error in errors[str(raw_status)]:
                    code = error["code"]
                    message = error["message"]
                    condition = error["condition"]
                    variants.append(
                        {
                            "allOf": [
                                copy.deepcopy(base_schema),
                                {
                                    "type": "object",
                                    "required": ["code", "message", "data"],
                                    "description": condition,
                                    "properties": {
                                        "code": {"type": "integer", "const": code},
                                        "message": {"type": "string", "const": message},
                                        "data": {"type": "null"},
                                    },
                                },
                            ]
                        }
                    )
                    examples[f"error_{code}"] = {
                        "summary": condition,
                        "value": {
                            "code": code,
                            "message": message,
                            "data": None,
                            "request_id": "req_bcn_001",
                        },
                    }
                media["schema"] = variants[0] if len(variants) == 1 else {"oneOf": variants}
                media["examples"] = examples
            operation["responses"][raw_status] = response


def inline_domain_schema_refs(
    value: Any,
    schemas: dict[str, Any],
    stack: tuple[str, ...] = (),
) -> Any:
    """Inline supporting domain schemas for the presentation-only model view."""

    if isinstance(value, dict):
        ref = value.get("$ref")
        if isinstance(ref, str):
            prefix = "#/components/schemas/"
            if not ref.startswith(prefix):
                raise ValueError(f"unsupported domain model view $ref: {ref}")
            name = ref.removeprefix(prefix)
            if name not in schemas:
                raise ValueError(f"unknown domain schema in model view: {name}")
            if name in stack:
                cycle = " -> ".join((*stack, name))
                raise ValueError(f"cyclic domain schema reference: {cycle}")

            resolved = inline_domain_schema_refs(
                copy.deepcopy(schemas[name]), schemas, (*stack, name)
            )
            siblings = {
                key: inline_domain_schema_refs(item, schemas, stack)
                for key, item in value.items()
                if key != "$ref"
            }
            if siblings:
                return {"allOf": [resolved], **siblings}
            return resolved

        result: dict[str, Any] = {}
        for key, item in value.items():
            if key == "discriminator" and isinstance(item, dict):
                # Mappings point at standalone supporting schemas, which are deliberately
                # absent from this projection. The discriminator field itself remains useful.
                item = {name: child for name, child in item.items() if name != "mapping"}
            result[key] = inline_domain_schema_refs(item, schemas, stack)
        return result
    if isinstance(value, list):
        return [inline_domain_schema_refs(item, schemas, stack) for item in value]
    return value


def validate_domain_model_view(spec: dict[str, Any]) -> None:
    schemas = (spec.get("components") or {}).get("schemas") or {}
    actual = list(schemas)
    expected = list(DOMAIN_MODEL_VIEW_SCHEMAS)
    if actual != expected:
        raise ValueError(
            f"domain model view schemas must be {expected}, got {actual}"
        )

    stack = [spec]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            if "$ref" in item:
                raise ValueError(
                    f"domain model view contains an unexpanded $ref: {item['$ref']}"
                )
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)


def build_domain_models() -> dict[str, Any]:
    source = rewrite_refs(copy.deepcopy(load_yaml(API_DIR / "domain-models.yaml")))
    validate_local_refs(source)
    validate_domain_fields(source)

    schemas = (source.get("components") or {}).get("schemas") or {}
    missing = [name for name in DOMAIN_MODEL_VIEW_SCHEMAS if name not in schemas]
    if missing:
        raise ValueError(f"domain model view schemas are missing: {missing}")

    spec = copy.deepcopy(source)
    spec["info"]["description"] = (
        "BCN 目标领域对象的展示视图。优先展示 Actor、Group、Session，"
        "其次展示 Provider 等支撑对象；枚举和从属结构内联到引用它们的领域对象中。"
    )
    spec["components"]["schemas"] = {
        name: inline_domain_schema_refs(
            copy.deepcopy(schemas[name]), schemas, (name,)
        )
        for name in DOMAIN_MODEL_VIEW_SCHEMAS
    }
    validate_domain_model_view(spec)
    return spec


def iter_operations(spec: dict[str, Any]):
    for route, path_item in (spec.get("paths") or {}).items():
        if not isinstance(path_item, dict):
            continue
        for method, operation in path_item.items():
            if method.lower() in METHODS and isinstance(operation, dict):
                yield route, method.lower(), operation


def validate_fragments() -> None:
    seen_operation_ids: dict[str, Path] = {}
    error_messages: dict[int, str] = {}
    for kind in ("openapi", "internalapi"):
        expected_prefix = "/openapi/v1/bcn/" if kind == "openapi" else "/api/v1/bcn/"
        expected_api_type = "openapi" if kind == "openapi" else "internal"
        for path in fragment_files(kind):
            spec = load_yaml(path)
            for schema_name, schema in (
                (spec.get("components") or {}).get("schemas") or {}
            ).items():
                validate_schema_property_descriptions(
                    schema, f"{path}:components/schemas/{schema_name}"
                )
            for route, method, operation in iter_operations(spec):
                label = f"{method.upper()} {route} in {path}"
                if not route.startswith(expected_prefix):
                    raise ValueError(f"invalid path prefix for {label}")
                operation_id = operation.get("operationId")
                if not isinstance(operation_id, str) or not operation_id:
                    raise ValueError(f"missing operationId for {label}")
                if operation_id in seen_operation_ids:
                    raise ValueError(
                        f"duplicate operationId {operation_id}: {seen_operation_ids[operation_id]} and {path}"
                    )
                seen_operation_ids[operation_id] = path
                if operation.get("x-api-type") != expected_api_type:
                    raise ValueError(f"invalid or missing x-api-type for {label}")
                if not isinstance(operation.get("description"), str) or not operation["description"].strip():
                    raise ValueError(f"missing description for {label}")
                if "security" not in operation:
                    raise ValueError(f"missing explicit security for {label}")
                responses = operation.get("responses")
                if not isinstance(responses, dict) or not responses:
                    raise ValueError(f"missing responses for {label}")
                validate_operation_error_codes(
                    operation, label, error_messages
                )
                for status, response in responses.items():
                    if isinstance(response, dict) and "$ref" not in response:
                        if not isinstance(response.get("description"), str):
                            raise ValueError(f"missing response description for {label} {status}")
                request_body = operation.get("requestBody")
                if isinstance(request_body, dict) and "required" not in request_body:
                    raise ValueError(f"requestBody.required not explicit for {label}")
                parameters = list(operation.get("parameters") or [])
                path_item = spec["paths"][route]
                parameters.extend(path_item.get("parameters") or [])
                for parameter in parameters:
                    if not isinstance(parameter, dict) or "$ref" in parameter:
                        continue
                    if "required" not in parameter:
                        raise ValueError(f"parameter.required not explicit for {label}")
                    if not isinstance(parameter.get("description"), str):
                        raise ValueError(f"parameter description missing for {label}")


def validate_operation_error_codes(
    operation: dict[str, Any],
    label: str,
    error_messages: dict[int, str],
) -> None:
    responses = operation.get("responses") or {}
    expected_statuses = {
        str(status)
        for status in responses
        if str(status).isdigit() and int(status) >= 300
    }
    declared = operation.get(ERROR_CODES_EXTENSION)
    if not isinstance(declared, dict):
        raise ValueError(f"missing {ERROR_CODES_EXTENSION} for {label}")
    declared_statuses = {str(status) for status in declared}
    if declared_statuses != expected_statuses:
        raise ValueError(
            f"{ERROR_CODES_EXTENSION} statuses for {label} must be "
            f"{sorted(expected_statuses)}, got {sorted(declared_statuses)}"
        )

    for raw_status, entries in declared.items():
        status = int(raw_status)
        if not isinstance(entries, list) or not entries:
            raise ValueError(f"empty error-code list for HTTP {status} in {label}")
        seen_codes: set[int] = set()
        for entry in entries:
            if not isinstance(entry, dict):
                raise ValueError(f"invalid error-code entry for HTTP {status} in {label}")
            code = entry.get("code")
            message = entry.get("message")
            condition = entry.get("condition")
            if not isinstance(code, int) or not 10000 <= code <= 59999:
                raise ValueError(f"error code must be a five-digit integer in {label}")
            if code // 100 != status:
                raise ValueError(
                    f"error code {code} does not match HTTP {status} in {label}"
                )
            if code in seen_codes:
                raise ValueError(f"duplicate error code {code} in {label}")
            seen_codes.add(code)
            if not isinstance(message, str) or not message.strip():
                raise ValueError(f"error code {code} lacks message in {label}")
            if not isinstance(condition, str) or not condition.strip():
                raise ValueError(f"error code {code} lacks condition in {label}")
            previous = error_messages.setdefault(code, message)
            if previous != message:
                raise ValueError(
                    f"error code {code} maps to both {previous!r} and {message!r}"
                )


def validate_bundle(spec: dict[str, Any], kind: str) -> None:
    validate_fragments()
    validate_local_refs(spec)
    declared_security = set((spec.get("components") or {}).get("securitySchemes") or {})
    for route, method, operation in iter_operations(spec):
        for requirement in operation.get("security") or []:
            for scheme in requirement:
                if scheme not in declared_security:
                    raise ValueError(
                        f"undeclared security scheme {scheme}: {method.upper()} {route} in {kind}"
                    )


def validate_local_refs(spec: dict[str, Any]) -> None:
    refs: list[str] = []
    stack = [spec]
    while stack:
        item = stack.pop()
        if isinstance(item, dict):
            ref = item.get("$ref")
            if isinstance(ref, str):
                if not ref.startswith("#/"):
                    raise ValueError(f"unresolved external $ref after bundling: {ref}")
                refs.append(ref)
            stack.extend(item.values())
        elif isinstance(item, list):
            stack.extend(item)

    for ref in refs:
        current: Any = spec
        for raw_part in ref[2:].split("/"):
            part = raw_part.replace("~1", "/").replace("~0", "~")
            if not isinstance(current, dict) or part not in current:
                raise ValueError(f"unresolved local $ref: {ref}")
            current = current[part]


def validate_domain_fields(spec: dict[str, Any]) -> None:
    schemas = (spec.get("components") or {}).get("schemas") or {}
    if not isinstance(schemas, dict) or not schemas:
        raise ValueError("domain-models.yaml has no schemas")

    def check_schema(schema: Any, location: str) -> None:
        if not isinstance(schema, dict):
            return
        properties = schema.get("properties")
        if isinstance(properties, dict):
            required = schema.get("required") or []
            if not isinstance(required, list):
                raise ValueError(f"required must be an array at {location}")
            for name in required:
                if name not in properties:
                    raise ValueError(f"required property {name} missing at {location}")
            for name, prop in properties.items():
                prop_location = f"{location}.{name}"
                if isinstance(prop, dict):
                    documented = isinstance(prop.get("description"), str) or "$ref" in prop
                    if not documented:
                        raise ValueError(f"domain property lacks description: {prop_location}")
                    check_schema(prop, prop_location)
        for keyword in ("allOf", "oneOf", "anyOf"):
            for index, child in enumerate(schema.get(keyword) or []):
                check_schema(child, f"{location}.{keyword}[{index}]")
        if isinstance(schema.get("items"), dict):
            check_schema(schema["items"], f"{location}.items")

    for name, schema in schemas.items():
        if not isinstance(schema, dict):
            raise ValueError(f"domain schema {name} is not an object")
        if not isinstance(schema.get("description"), str) and "$ref" not in schema:
            raise ValueError(f"domain schema lacks description: {name}")
        check_schema(schema, name)


def validate_schema_property_descriptions(schema: Any, location: str) -> None:
    """Require every explicitly declared schema property to explain its meaning."""

    if not isinstance(schema, dict):
        return
    properties = schema.get("properties")
    if isinstance(properties, dict):
        for name, prop in properties.items():
            prop_location = f"{location}.{name}"
            if isinstance(prop, dict):
                documented = isinstance(prop.get("description"), str) or "$ref" in prop
                if not documented:
                    raise ValueError(f"schema property lacks description: {prop_location}")
                validate_schema_property_descriptions(prop, prop_location)
    for keyword in ("allOf", "oneOf", "anyOf"):
        for index, child in enumerate(schema.get(keyword) or []):
            validate_schema_property_descriptions(
                child, f"{location}.{keyword}[{index}]"
            )
    if isinstance(schema.get("items"), dict):
        validate_schema_property_descriptions(schema["items"], f"{location}.items")


def count_operations(spec: dict[str, Any]) -> int:
    return sum(1 for _ in iter_operations(spec))


def render_yaml(spec: dict[str, Any]) -> bytes:
    return yaml.safe_dump(spec, allow_unicode=True, sort_keys=False).encode("utf-8")


def render_html(default_spec: str = "all") -> bytes:
    html = f"""<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>BCN API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  <style>
    body {{ margin: 0; background: #f7f8fa; }}
    #header {{ padding: 16px 24px; background: #111827; color: #fff; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; display: flex; align-items: center; justify-content: space-between; gap: 24px; flex-wrap: wrap; }}
    #header h1 {{ margin: 0 0 4px; font-size: 20px; font-weight: 650; }}
    #header p {{ margin: 0; color: #cbd5e1; font-size: 13px; }}
    #api-switcher {{ display: flex; align-items: center; gap: 10px; color: #cbd5e1; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; font-size: 13px; white-space: nowrap; }}
    #api-spec-selector {{ min-width: 230px; border: 1px solid #475569; border-radius: 6px; background: #fff; color: #111827; font-size: 14px; padding: 7px 10px; }}
  </style>
</head>
<body>
  <div id="header">
    <div>
      <h1>BCN API Docs</h1>
      <p>按共同业务类目查看全部、OpenAPI、Internal API 或领域对象。</p>
    </div>
    <label id="api-switcher" for="api-spec-selector">
      文档视图
      <select id="api-spec-selector">
        <option value="all">BCN All APIs</option>
        <option value="openapi">BCN OpenAPI</option>
        <option value="internalapi">BCN Internal API</option>
        <option value="domain-models">BCN Domain Models</option>
      </select>
    </label>
  </div>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
  <script>
    window.onload = () => {{
      const specs = {{
        all: {{ url: '/all-apis.yaml', name: 'BCN All APIs' }},
        openapi: {{ url: '/openapi.yaml', name: 'BCN OpenAPI' }},
        internalapi: {{ url: '/internalapi.yaml', name: 'BCN Internal API' }},
        'domain-models': {{ url: '/domain-models.yaml', name: 'BCN Domain Models' }},
      }};
      const params = new URLSearchParams(window.location.search);
      const requestedSpec = params.get('spec');
      const fallback = '{default_spec}';
      const selectedKey = specs[requestedSpec] ? requestedSpec : fallback;
      const selector = document.getElementById('api-spec-selector');
      selector.value = selectedKey;
      selector.addEventListener('change', (event) => {{
        const nextUrl = new URL(window.location.origin + '/');
        nextUrl.searchParams.set('spec', event.target.value);
        window.location.href = nextUrl.toString();
      }});
      document.title = `${{specs[selectedKey].name}} - BCN API Docs`;
      SwaggerUIBundle({{
        dom_id: '#swagger-ui',
        url: specs[selectedKey].url,
        deepLinking: true,
        defaultModelsExpandDepth: 2,
        defaultModelExpandDepth: 3,
        docExpansion: 'list',
        presets: [SwaggerUIBundle.presets.apis, SwaggerUIStandalonePreset],
        layout: 'StandaloneLayout',
        tryItOutEnabled: false,
      }});
    }};
  </script>
</body>
</html>
"""
    return html.encode("utf-8")


class ApiDocsHandler(BaseHTTPRequestHandler):
    server_version = "BcnNewApiDocs/1.0"

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        parsed = urlparse(self.path)
        path = parsed.path
        specs = self.server.specs  # type: ignore[attr-defined]
        if path in {"/", "/index.html"}:
            requested = parse_qs(parsed.query).get("spec", ["all"])[0]
            default_spec = requested if requested in SPEC_KEYS else "all"
            self.respond(200, "text/html; charset=utf-8", render_html(default_spec))
        elif path in {"/domain-models", "/domain-models/"}:
            self.respond(200, "text/html; charset=utf-8", render_html("domain-models"))
        elif path == "/all-apis.yaml":
            self.respond(200, "application/yaml; charset=utf-8", render_yaml(specs["all"]))
        elif path == "/openapi.yaml":
            self.respond(200, "application/yaml; charset=utf-8", render_yaml(specs["openapi"]))
        elif path == "/internalapi.yaml":
            self.respond(200, "application/yaml; charset=utf-8", render_yaml(specs["internalapi"]))
        elif path == "/domain-models.yaml":
            self.respond(200, "application/yaml; charset=utf-8", render_yaml(specs["domain-models"]))
        elif path == "/all-apis.json":
            self.respond_json(200, specs["all"])
        elif path == "/openapi.json":
            self.respond_json(200, specs["openapi"])
        elif path == "/internalapi.json":
            self.respond_json(200, specs["internalapi"])
        elif path == "/domain-models.json":
            self.respond_json(200, specs["domain-models"])
        elif path == "/healthz":
            self.respond_json(200, {"ok": True})
        elif path == "/favicon.ico":
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


def build_specs() -> dict[str, dict[str, Any]]:
    validate_fragments()
    return {
        "all": build_api_bundle("all"),
        "openapi": build_api_bundle("openapi"),
        "internalapi": build_api_bundle("internalapi"),
        "domain-models": build_domain_models(),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Serve BCN target API docs with Swagger UI.")
    parser.add_argument("--host", default="127.0.0.1", help="bind host")
    parser.add_argument("--port", type=int, default=8766, help="bind port")
    parser.add_argument("--check", action="store_true", help="validate and build all specs, then exit")
    parser.add_argument("--open", action="store_true", help="open the docs page in a browser")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    specs = build_specs()

    if args.check:
        for name in SPEC_KEYS:
            spec = specs[name]
            schemas = len((spec.get("components") or {}).get("schemas") or {})
            print(
                f"{name}: paths={len(spec.get('paths') or {})} "
                f"operations={count_operations(spec)} schemas={schemas}"
            )
        return 0

    server = ThreadingHTTPServer((args.host, args.port), ApiDocsHandler)
    server.specs = specs  # type: ignore[attr-defined]
    host, port = server.server_address
    url = f"http://{host}:{port}/"
    print(f"BCN new API docs server listening on {url}")
    print("Specs: /all-apis.yaml, /openapi.yaml, /internalapi.yaml, /domain-models.yaml")
    if args.open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping BCN new API docs server.")
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
