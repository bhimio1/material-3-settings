#!/usr/bin/env python3
"""Generate assets/end4/schema.json from end4-pC's modules/common/Config.qml.

Usage: gen_end4_schema.py path/to/Config.qml > assets/end4/schema.json

Each option becomes {"key", "type", "default", "comment"}. List options keep
their default as raw JSON when it parses, otherwise null.
"""
import json
import re
import sys

PROP = re.compile(r"^\s*property\s+(\S+)\s+(\w+)\s*:\s*(.*)$")


def strip_comment(s):
    """Split `value // comment`, ignoring // inside strings."""
    in_str = None
    i = 0
    while i < len(s):
        c = s[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == in_str:
                in_str = None
        elif c in "\"'":
            in_str = c
        elif s.startswith("//", i):
            return s[:i].strip(), s[i + 2 :].strip()
        i += 1
    return s.strip(), ""


def to_json_value(qml_type, raw):
    raw = raw.rstrip(";").strip()
    if qml_type == "bool":
        return raw == "true" if raw in ("true", "false") else None
    if qml_type in ("int", "real"):
        try:
            v = float(raw)
            return int(v) if qml_type == "int" and v.is_integer() else v
        except ValueError:
            return None
    if qml_type == "string":
        try:
            return json.loads(raw)
        except Exception:
            return raw.strip("\"'") if raw[:1] in "\"'" else None
    try:
        return json.loads(raw)
    except Exception:
        return None


def main(path):
    lines = open(path, encoding="utf-8").read().splitlines()
    stack = []
    out = []
    pending_comment = []
    i = 0
    started = False
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        if "JsonAdapter" in line and "{" in line:
            started = True
            i += 1
            continue
        if not started:
            i += 1
            continue
        if stripped.startswith("//"):
            pending_comment.append(stripped[2:].strip())
            i += 1
            continue
        m = PROP.match(line)
        if m:
            qml_type, name, rest = m.groups()
            value, comment = strip_comment(rest)
            if not comment and pending_comment:
                comment = " ".join(pending_comment)
            pending_comment = []
            if qml_type == "JsonObject":
                stack.append(name)
                i += 1
                continue
            # Multi-line value (lists, objects): collect until brackets balance.
            depth = value.count("[") + value.count("{") - value.count("]") - value.count("}")
            raw = [value]
            while depth > 0 and i + 1 < len(lines):
                i += 1
                v, _ = strip_comment(lines[i])
                raw.append(v)
                depth += v.count("[") + v.count("{") - v.count("]") - v.count("}")
            raw_value = " ".join(raw)
            t = {"bool": "bool", "int": "int", "real": "real", "string": "string"}.get(qml_type, "list" if qml_type.startswith("list") or qml_type == "var" else qml_type)
            out.append({
                "key": ".".join(stack + [name]),
                "type": t,
                "default": to_json_value(qml_type, raw_value),
                "comment": comment,
            })
            i += 1
            continue
        if stripped.startswith("}"):
            pending_comment = []
            if stack:
                stack.pop()
            else:
                break
        elif stripped:
            pending_comment = []
        i += 1
    json.dump(out, sys.stdout, indent=1, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main(sys.argv[1])
