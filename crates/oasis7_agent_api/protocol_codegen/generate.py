#!/usr/bin/env python3
"""Development-only, deterministic signed-read profile compiler (Python stdlib)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
MANAGED = {"types.ts", "schema.json", "manifest.json"}
RESERVED = {"Operations", "Array"} | set("as any unknown never number bigint boolean string symbol object undefined null true false void this typeof keyof readonly infer extends new class function interface type enum export import default declare namespace module implements package private protected public static yield await let const var delete in instanceof return throw try catch finally switch case break continue do else for if while with debugger super constructor arguments eval".split())
KEYS = {"$ref", "type", "properties", "required", "additionalProperties", "items", "anyOf", "allOf", "oneOf", "not", "const", "enum", "pattern", "minLength", "maxLength", "minimum", "maximum", "description", "title"}

class Invalid(ValueError):
    pass

def digest(data):
    return hashlib.sha256(data).hexdigest()

def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2, allow_nan=False) + "\n").encode()

def load(data):
    if len(data) > 2097152:
        raise Invalid("JSON input capacity exceeded")
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise Invalid("duplicate JSON key: " + key)
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=pairs, parse_constant=lambda x: (_ for _ in ()).throw(Invalid("nonfinite JSON")))

def safe(root, relative):
    path = Path(relative)
    if path.is_absolute() or ".." in path.parts:
        raise Invalid("path outside repository: " + str(path))
    current = root
    for part in path.parts:
        current = current / part
        if current.is_symlink():
            raise Invalid("symlink: " + str(path))
    return current

def bounded(value, limits, depth=0, count=None):
    count = [0] if count is None else count
    count[0] += 1
    if depth > limits["depth"] or count[0] > limits["nodes"]:
        raise Invalid("schema depth/node capacity exceeded")
    if isinstance(value, dict):
        for child in value.values():
            bounded(child, limits, depth + 1, count)
    elif isinstance(value, list):
        for child in value:
            bounded(child, limits, depth + 1, count)

def validate_pattern(pattern):
    """Small explicit ECMAScript-u grammar; extend only with tested dialect support."""
    index = 0
    groups = []
    atom = False
    quantified = False
    while index < len(pattern):
        char = pattern[index]
        if pattern.startswith(r"(?![\s\S])", index):
            if index + len(r"(?![\s\S])") != len(pattern):
                raise Invalid("unsupported non-ECMAScript regex construct")
            index = len(pattern)
            continue
        if pattern.startswith("(?:", index):
            groups.append(index)
            index += 3
            atom = False
            quantified = False
            continue
        if char == "[":
            end = pattern.find("]", index + 1)
            if end < 0 or end == index + 1:
                raise Invalid("unsupported non-ECMAScript regex class")
            content = pattern[index + 1:end]
            if not re.fullmatch(r"[A-Za-z0-9-]+", content):
                raise Invalid("unsupported non-ECMAScript regex class")
            index = end + 1
            atom, quantified = True, False
            continue
        if char == ")":
            if not groups or not atom:
                raise Invalid("unsupported non-ECMAScript regex group")
            groups.pop()
            atom, quantified = True, False
        elif char == "|":
            if not atom:
                raise Invalid("unsupported non-ECMAScript regex alternation")
            atom, quantified = False, False
        elif char in "*+?{":
            if not atom or quantified:
                raise Invalid("unsupported non-ECMAScript regex quantifier")
            if char == "{":
                match = re.match(r"\{([0-9]+)(?:,([0-9]+))?\}", pattern[index:])
                if not match or (match[2] is not None and int(match[1]) > int(match[2])):
                    raise Invalid("unsupported non-ECMAScript regex quantifier")
                index += len(match[0]) - 1
            quantified = True
        elif char in "^$":
            if (char == "^" and index != 0) or (char == "$" and index != len(pattern) - 1):
                raise Invalid("unsupported non-ECMAScript regex anchor")
            atom, quantified = False, False
        elif char.isascii() and (char.isalnum() or char in "_:./-"):
            atom, quantified = True, False
        else:
            raise Invalid("unsupported non-ECMAScript regex construct")
        index += 1
    if groups:
        raise Invalid("unsupported non-ECMAScript regex unclosed group")
    re.compile(pattern)

def validate_schema(schema, names):
    if not isinstance(schema, dict):
        raise Invalid("only object schemas supported")
    for key in ("description", "title", "$ref", "pattern"):
        if key in schema and not isinstance(schema[key], str):
            raise Invalid("invalid schema " + key)
    for key in ("minLength", "maxLength"):
        if key in schema and (type(schema[key]) is not int or schema[key] < 0):
            raise Invalid("invalid schema " + key)
    for key in ("minimum", "maximum"):
        if key in schema and type(schema[key]) is not int:
            raise Invalid("only exact integer schema bounds supported: " + key)
    if "minLength" in schema and "maxLength" in schema and schema["minLength"] > schema["maxLength"]:
        raise Invalid("invalid schema length range")
    if "minimum" in schema and "maximum" in schema and schema["minimum"] > schema["maximum"]:
        raise Invalid("invalid schema numeric range")
    for key in ("anyOf", "allOf", "oneOf", "enum"):
        if key in schema and (not isinstance(schema[key], list) or not schema[key]):
            raise Invalid("invalid schema " + key)
    literals = schema.get("enum", []) + ([schema["const"]] if "const" in schema else [])
    if any(type(value) not in (str, int, bool, type(None)) or (type(value) is int and abs(value) > 9007199254740991) for value in literals):
        raise Invalid("unsupported nonprimitive or unsafe numeric type literal")
    if "enum" in schema and len({json.dumps(x, sort_keys=True) for x in schema["enum"]}) != len(schema["enum"]):
        raise Invalid("duplicate enum value")
    if "required" in schema and (not isinstance(schema["required"], list) or any(not isinstance(x, str) for x in schema["required"]) or len(set(schema["required"])) != len(schema["required"])):
        raise Invalid("invalid schema required")
    if "properties" in schema and not isinstance(schema["properties"], dict):
        raise Invalid("invalid schema properties")
    unknown = set(schema) - KEYS
    if unknown:
        raise Invalid("unsupported schema keywords: " + ", ".join(sorted(unknown)))
    if "$ref" in schema:
        ref = schema["$ref"]
        if not ref.startswith("#/components/schemas/") or ref.removeprefix("#/components/schemas/") not in names:
            raise Invalid("invalid local $ref: " + ref)
    if "type" in schema:
        types = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        if not types or any(not isinstance(x, str) for x in types) or len(set(types)) != len(types) or set(types) - {"object", "array", "string", "integer", "number", "boolean", "null"}:
            raise Invalid("unsupported schema type")
    for keyword in ("properties",):
        for value in schema.get(keyword, {}).values():
            validate_schema(value, names)
    for keyword in ("allOf", "anyOf", "oneOf"):
        for value in schema.get(keyword, []):
            validate_schema(value, names)
    for keyword in ("items", "not"):
        if keyword in schema:
            validate_schema(schema[keyword], names)
    additional = schema.get("additionalProperties", True)
    if not isinstance(additional, bool):
        validate_schema(additional, names)
    if "pattern" in schema:
        validate_pattern(schema["pattern"])
    if set(schema.get("required", [])) - set(schema.get("properties", {})):
        raise Invalid("required property not declared")

def validate_reference_graph(schemas, maximum_depth):
    def references(value):
        result = set()
        if isinstance(value, dict):
            if "$ref" in value:
                result.add(value["$ref"].split("/")[-1])
            for child in value.values():
                result.update(references(child))
        elif isinstance(value, list):
            for child in value:
                result.update(references(child))
        return result
    graph = {name: references(schema) for name, schema in schemas.items()}
    done = set()
    def visit(name, active):
        if name in active:
            raise Invalid("unsupported recursive component reference: " + name)
        if len(active) >= maximum_depth:
            raise Invalid("component reference depth capacity exceeded")
        if name in done:
            return
        for child in graph[name]:
            visit(child, active | {name})
        done.add(name)
    for name in graph:
        visit(name, set())

def ts(schema, budget=None):
    budget = [4194304] if budget is None else budget
    parts = []
    if "$ref" in schema:
        parts.append(schema["$ref"].split("/")[-1])
    if "const" in schema:
        parts.append(json.dumps(schema["const"]))
    if "enum" in schema:
        parts.append("(" + " | ".join(json.dumps(x) for x in schema["enum"]) + ")")
    for key, sep in (("allOf", " & "), ("anyOf", " | "), ("oneOf", " | ")):
        if key in schema:
            parts.append("(" + sep.join(ts(x, budget) for x in schema[key]) + ")")
    kind = schema.get("type")
    if isinstance(kind, list):
        # Union only the type-specific shape; common const/combinators above
        # are emitted once, avoiding exponential replication of shared clauses.
        variants = []
        for item in kind:
            shape = {"type": item}
            if item == "object":
                shape.update({key: schema[key] for key in ("properties", "required", "additionalProperties") if key in schema})
            elif item == "array" and "items" in schema:
                shape["items"] = schema["items"]
            variants.append(ts(shape, budget))
        parts.append("(" + " | ".join(variants) + ")")
    elif kind == "object" or (kind is None and "properties" in schema):
        fields = [json.dumps(k) + ("" if k in schema.get("required", []) else "?") + ": " + ts(v, budget) + ";" for k, v in sorted(schema.get("properties", {}).items())]
        if schema.get("additionalProperties", True) is not False:
            fields.append("[key: string]: unknown;")
        parts.append("{ " + " ".join(fields) + " }")
    elif kind == "array":
        parts.append("Array<" + ts(schema.get("items", {}), budget) + ">")
    elif kind:
        parts.append({"integer": "number", "number": "number", "string": "string", "boolean": "boolean", "null": "null"}[kind])
    result = " & ".join(parts) if parts else "unknown"
    budget[0] -= len(result.encode())
    if budget[0] < 0:
        raise Invalid("TypeScript expansion capacity exceeded")
    return result

def rewrite(value):
    if isinstance(value, dict):
        return {key: (child.replace("#/components/schemas/", "#/$defs/") if key == "$ref" else rewrite(child)) for key, child in value.items()}
    if isinstance(value, list):
        return [rewrite(x) for x in value]
    return value

def generate(root=ROOT):
    base = HERE.relative_to(ROOT)
    config_path = safe(root, base / "profile.json")
    if config_path.stat().st_size > 65536:
        raise Invalid("profile capacity exceeded")
    profile = load(config_path.read_bytes())
    if set(profile) != {"profile", "format_version", "source", "output", "export_roots", "target", "features", "limits"}:
        raise Invalid("unsupported profile configuration")
    if profile["format_version"] != 1 or profile["target"] != "http-json" or profile["features"] != [] or profile["export_roots"] != ["components/schemas", "paths"]:
        raise Invalid("unsupported profile configuration")
    if profile["profile"] != "signed-read-bootstrap" or profile["source"] != "crates/oasis7_agent_api/src/game/read_bootstrap.openapi.json" or profile["output"] != str(base / "generated"):
        raise Invalid("profile source/output outside explicit managed paths")
    limits = profile["limits"]
    maxima = {"input_bytes": 2097152, "output_bytes": 4194304, "depth": 64, "nodes": 50000, "timeout_seconds": 10}
    if set(limits) != set(maxima) or any(type(value) is not int or value < 1 or value > maxima[key] for key, value in limits.items()):
        raise Invalid("invalid profile capacity limits")
    paths = [profile["source"], str(base / "profile.json"), str(base / "toolchain.lock.json"), str(base / "generate.py")]
    inputs = {}
    for relative in paths:
        path = safe(root, relative)
        if path.stat().st_size > limits["input_bytes"]:
            raise Invalid("input capacity exceeded: " + relative)
        data = path.read_bytes()
        if len(data) > limits["input_bytes"]:
            raise Invalid("input capacity exceeded: " + relative)
        inputs[relative] = data
    lock = load(inputs[str(base / "toolchain.lock.json")])
    if set(lock) != {"generator", "python_major_minor", "dependencies", "schema_dialect"} or lock["generator"] != "python-stdlib" or lock["dependencies"] != {}:
        raise Invalid("unsupported toolchain lock")
    if list(sys.version_info[:2]) != lock["python_major_minor"]:
        raise Invalid("locked Python major/minor required: " + str(lock["python_major_minor"]))
    source = load(inputs[profile["source"]])
    bounded(source, limits)
    if source["openapi"] != "3.1.0":
        raise Invalid("unsupported OpenAPI version")
    allowed_source = {"openapi", "jsonSchemaDialect", "info", "paths", "components", "x-oasis7-api-revision", "x-oasis7-schema-revision", "x-oasis7-limits"}
    if set(source) - allowed_source or set(source["components"]) - {"schemas", "securitySchemes"}:
        raise Invalid("unsupported OpenAPI root/component construct")
    if source["jsonSchemaDialect"] != lock["schema_dialect"]:
        raise Invalid("unsupported schema dialect")
    schemas = source["components"]["schemas"]
    for name, schema in schemas.items():
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) or name in RESERVED:
            raise Invalid("invalid exported name")
        validate_schema(schema, schemas)
    validate_reference_graph(schemas, limits["depth"])
    operations = {}
    for security in source["components"].get("securitySchemes", {}).values():
        if set(security) - {"type", "in", "name", "description", "x-oasis7-decoded-schema", "x-oasis7-operation-domain"}:
            raise Invalid("unsupported security scheme construct")
        if "x-oasis7-decoded-schema" in security:
            validate_schema(security["x-oasis7-decoded-schema"], schemas)
    for route, methods in sorted(source["paths"].items()):
        if set(methods) != {"get"}:
            raise Invalid("profile only supports GET")
        operation = methods["get"]
        if set(operation) - {"operationId", "description", "security", "parameters", "responses"}:
            raise Invalid("unsupported operation construct")
        for param in operation.get("parameters", []):
            if set(param) - {"name", "in", "required", "schema"}:
                raise Invalid("unsupported parameter construct")
            validate_schema(param["schema"], schemas)
        responses = {}
        for status, response in operation["responses"].items():
            if set(response) - {"description", "content"} or set(response["content"]) != {"application/json"}:
                raise Invalid("unsupported response construct")
            schema = response["content"]["application/json"]["schema"]
            validate_schema(schema, schemas)
            responses[status] = schema
        name = operation["operationId"]
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) or name in RESERVED:
            raise Invalid("invalid operationId")
        if name in operations:
            raise Invalid("duplicate operationId")
        operations[name] = {"method": "GET", "path": route, "responses": responses}
    if len(operations) != 5:
        raise Invalid("profile requires exactly five GET roots")
    lines = ["// Generated by protocol_codegen/generate.py; do not edit.", "// Runtime schema validation is required; types do not encode integer ranges."]
    ts_budget = [limits["output_bytes"]]
    lines += ["export type " + name + " = " + ts(schema, ts_budget) + ";" for name, schema in sorted(schemas.items())]
    lines += ["export interface Operations {"]
    for name, operation in sorted(operations.items()):
        lines.append("  " + json.dumps(name) + ": { responses: { " + " ".join(json.dumps(status) + ": " + ts(schema, ts_budget) + ";" for status, schema in sorted(operation["responses"].items())) + " } };")
    lines += ["}", ""]
    schema = {"$comment": "Generated by crates/oasis7_agent_api/protocol_codegen/generate.py; edit " + profile["source"] + " and regenerate.", "$schema": source["jsonSchemaDialect"], "$defs": rewrite(schemas), "x-operations": rewrite(operations)}
    outputs = {"types.ts": "\n".join(lines).encode(), "schema.json": encoded(schema)}
    manifest = {"format_version": profile["format_version"], "profile": profile["profile"], "api_revision": source["x-oasis7-api-revision"], "schema_revision": source["x-oasis7-schema-revision"], "wire_version": source["info"]["version"], "export_roots": profile["export_roots"], "target": profile["target"], "features": profile["features"], "inputs": {k: digest(v) for k, v in sorted(inputs.items())}, "outputs": {k: digest(v) for k, v in sorted(outputs.items())}}
    outputs["manifest.json"] = encoded(manifest)
    # Verify the complete staged representation before publishing any file.
    staged_schema = load(outputs["schema.json"])
    if rewrite(source["components"]["schemas"]) != staged_schema["$defs"]:
        raise Invalid("staged schema differs from authority")
    def refs(value):
        if isinstance(value, dict):
            for key, child in value.items():
                if key == "$ref" and (not child.startswith("#/$defs/") or child.removeprefix("#/$defs/") not in staged_schema["$defs"]):
                    raise Invalid("invalid staged reference")
                refs(child)
        elif isinstance(value, list):
            for child in value:
                refs(child)
    refs(staged_schema)
    if load(outputs["manifest.json"])["outputs"] != {k: digest(v) for k, v in sorted(outputs.items()) if k != "manifest.json"}:
        raise Invalid("invalid staged manifest")
    if sum(map(len, outputs.values())) > limits["output_bytes"]:
        raise Invalid("output capacity exceeded")
    return profile, inputs, outputs

def snapshot(directory):
    if not directory.exists():
        return {}
    result = {}
    for path in directory.iterdir():
        if path.is_symlink() or not path.is_file():
            raise Invalid("invalid managed output: " + path.name)
        if path.stat().st_size > 4194304 or len(result) >= 16:
            raise Invalid("managed output capacity exceeded")
        result[path.name] = path.read_bytes()
    return result

def run(mode, root=ROOT, before_commit=None):
    profile, inputs, outputs = generate(root)
    directory = safe(root, profile["output"])
    previous = snapshot(directory)
    diagnostics = ["extra: " + x for x in sorted(set(previous) - MANAGED)]
    diagnostics += [("missing: " if name not in previous else "stale: ") + name for name, data in sorted(outputs.items()) if previous.get(name) != data]
    if mode == "check":
        if diagnostics:
            raise Invalid("\n".join(diagnostics))
        return
    directory.parent.mkdir(parents=True, exist_ok=True)
    lock = directory.parent / ".generate.lock"
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as exc:
        raise Invalid("generation write lock exists; inspect interrupted writer before removing") from exc
    try:
        os.close(fd)
        with tempfile.TemporaryDirectory(prefix=".generate-", dir=directory.parent) as temporary:
            staging = Path(temporary)
            for name, data in outputs.items():
                (staging / name).write_bytes(data)
            if before_commit:
                before_commit()
            for relative, data in inputs.items():
                if safe(root, relative).read_bytes() != data:
                    raise Invalid("input preimage conflict: " + relative)
            safe(root, profile["output"])
            if snapshot(directory) != previous:
                raise Invalid("output preimage conflict")
            if set(previous) - MANAGED:
                raise Invalid("extra output prevents write")
            directory.mkdir(exist_ok=True)
            # Multiple renames are deliberately not an atomic publication. check rejects mixtures.
            for name in ["schema.json", "types.ts", "manifest.json"]:
                os.replace(staging / name, directory / name)
    finally:
        lock.unlink()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["write", "check"])
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    try:
        if not args.worker:
            profile_path = HERE / "profile.json"
            if profile_path.is_symlink() or profile_path.stat().st_size > 65536:
                raise Invalid("invalid profile path/capacity")
            profile = load(profile_path.read_bytes())
            seconds = profile["limits"]["timeout_seconds"]
            if type(seconds) is not int or not 1 <= seconds <= 10:
                raise Invalid("invalid profile timeout")
            try:
                return subprocess.run([sys.executable, str(HERE / "generate.py"), args.mode, "--worker"], timeout=seconds, check=False).returncode
            except subprocess.TimeoutExpired as exc:
                raise Invalid("generation timeout exceeded; inspect write lock before recovery") from exc
        run(args.mode)
        print("signed-read-bootstrap: " + args.mode + " passed")
    except (ValueError, KeyError, OSError, TypeError, RecursionError) as exc:
        print("invalid: " + str(exc), file=sys.stderr)
        return 1
    return 0

if __name__ == "__main__":
    sys.exit(main())
