#!/usr/bin/env python3
"""Generate `custom_data.rs` (the ~143 custom extractors) from upstream
`src/extractors/custom/<domain>/index.js`.

Usage: python tools/generate_custom_extractors.py <upstream-custom-dir> <output.rs> <named.txt>

The upstream JS files are structurally uniform: each exports an object with
`domain`, optional `supportedDomains`, and field objects with `selectors`
(plus `format`/`timezone` for date_published, and `clean`/`transforms`/
`defaultCleaner` for content). Transform functions are named
`<domain>::<selector>` and implemented by hand in
`src/extractors/transforms/sites.rs`.
"""

import os
import re
import sys


class ParseError(Exception):
    pass


def skip_ws(s, i):
    while i < len(s) and s[i] in " \t\r\n":
        i += 1
    return i


def parse_string(s, i):
    """Parse a JS string starting at s[i] (either quote). Returns (value, end)."""
    quote = s[i]
    i += 1
    out = []
    while i < len(s):
        c = s[i]
        if c == "\\":
            nxt = s[i + 1] if i + 1 < len(s) else ""
            out.append(nxt)
            i += 2
            continue
        if c == quote:
            return "".join(out), i + 1
        out.append(c)
        i += 1
    raise ParseError("unterminated string")


def parse_raw_block(s, i):
    """Consume an unparseable code block up to its matching closing brace."""
    start = i
    depth = 0
    in_str = None
    prev = None  # last significant char, for regex-vs-division detection
    while i < len(s):
        c = s[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == in_str:
                in_str = None
            i += 1
            continue
        if s.startswith("//", i):
            nl = s.find("\n", i)
            i = len(s) if nl == -1 else nl
            continue
        if s.startswith("/*", i):
            close = s.find("*/", i + 2)
            i = len(s) if close == -1 else close + 2
            continue
        if c in "'\"`":
            in_str = c
        elif c == "/" and prev in "=(,:;!&|?{[-":
            # regex literal: scan to the unescaped closing `/`
            i += 1
            in_class = False
            while i < len(s):
                rc = s[i]
                if rc == "\\":
                    i += 2
                    continue
                if rc == "[":
                    in_class = True
                elif rc == "]":
                    in_class = False
                elif rc == "/" and not in_class:
                    break
                i += 1
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth <= 0:
                return ("raw", s[start:i + 1]), i + 1
        if not c.isspace():
            prev = c
        i += 1
    raise ParseError("unterminated raw block")


def parse_value(s, i):
    """Parse a JS value: str / list / dict / bool / None / ('raw', text)."""
    i = skip_ws(s, i)
    if i >= len(s):
        raise ParseError("unexpected end")
    c = s[i]
    if c in "'\"":
        return parse_string(s, i)
    if c == "[":
        return parse_array(s, i)
    if c == "{":
        return parse_object(s, i)
    if s.startswith("function", i) or s.startswith("=>", i) or c == "(":
        # transform function `($node, $) => { ... }` / `function () { ... }`
        return parse_raw_block(s, i)
    if c == "$":
        # transform function `$node => { ... }`
        return parse_raw_block(s, i)
    m = re.match(r"[A-Za-z0-9_.\-]+", s[i:])
    tok = m.group(0) if m else ""
    if tok == "true":
        return True, i + len(tok)
    if tok == "false":
        return False, i + len(tok)
    if tok == "null":
        return None, i + len(tok)
    if tok == "":
        raise ParseError("cannot parse value at %r" % (s[i:i + 20],))
    # bare identifier; an arrow function continues with `=> { ... }`
    j = skip_ws(s, i + len(tok))
    if s.startswith("=>", j):
        return parse_raw_block(s, j)
    return ("raw", tok), i + len(tok)


def parse_array(s, i):
    i += 1  # [
    out = []
    while True:
        i = skip_ws(s, i)
        if i >= len(s):
            raise ParseError("unterminated array")
        if s[i] == "]":
            return out, i + 1
        if s[i] == ",":
            i += 1
            continue
        if s.startswith("//", i):
            nl = s.find("\n", i)
            i = len(s) if nl == -1 else nl
            continue
        val, i = parse_value(s, i)
        out.append(val)
        i = skip_ws(s, i)
        if i < len(s) and s[i] == ",":
            i += 1


def parse_object(s, i):
    i += 1  # {
    out = {}
    while True:
        i = skip_ws(s, i)
        if i >= len(s):
            raise ParseError("unterminated object")
        if s[i] == "}":
            return out, i + 1
        if s.startswith("//", i):
            nl = s.find("\n", i)
            i = len(s) if nl == -1 else nl
            continue
        if s[i] in "'\"":
            key, i = parse_string(s, i)
        else:
            m = re.match(r"[A-Za-z_$][A-Za-z0-9_$]*", s[i:])
            if not m:
                raise ParseError("cannot parse key at %r" % (s[i:i + 20],))
            key = m.group(0)
            i += len(key)
        i = skip_ws(s, i)
        if i >= len(s) or s[i] != ":":
            raise ParseError("expected ':' after key %r" % (key,))
        i += 1
        val, i = parse_value(s, i)
        out[key] = val
        i = skip_ws(s, i)
        if i < len(s) and s[i] == ",":
            i += 1


def parse_extractor(src, domain_dir):
    m = re.search(r"export\s+const\s+\w+\s*=\s*(\{)", src)
    if not m:
        raise ParseError("%s: no export const object" % domain_dir)
    obj, _ = parse_object(src, m.start(1))
    return obj


def to_rust_string(v):
    return json_dumps(v)


def json_dumps(v):
    import json

    return json.dumps(v, ensure_ascii=False)


def selectors_to_rust(selectors, is_content):
    items = []
    for sel in selectors:
        if sel is None:
            continue
        if isinstance(sel, str):
            items.append("Selector::Css(%s.into())" % json_dumps(sel))
        elif isinstance(sel, list) and len(sel) >= 2 and all(isinstance(x, str) for x in sel[:2]):
            if is_content:
                inner = ", ".join("%s.into()" % json_dumps(x) for x in sel)
                items.append("Selector::Multi(vec![%s])" % inner)
            else:
                attr = json_dumps(sel[1])
                transform = "None"
                if len(sel) > 2 and isinstance(sel[2], str):
                    transform = "Some(%s.into())" % json_dumps(sel[2])
                items.append(
                    "Selector::Attr { selector: %s.into(), attr: %s.into(), transform: %s }"
                    % (json_dumps(sel[0]), attr, transform)
                )
        elif isinstance(sel, list):
            inner = ", ".join("%s.into()" % json_dumps(x) for x in sel if isinstance(x, str))
            items.append("Selector::Multi(vec![%s])" % inner)
        else:
            raise ParseError("raw selector: %r" % (sel,))
    return items


def field_to_rust(key, value):
    if value is None:
        return []
    if isinstance(value, str):
        return ["FieldValue::Value(%s.into())" % json_dumps(value)]
    if not isinstance(value, dict):
        raise ParseError("bad field %s: %r" % (key, value))
    selectors = value.get("selectors", [])
    if not isinstance(selectors, list):
        raise ParseError("bad selectors for %s: %r" % (key, selectors))
    return selectors_to_rust(selectors, key == "content")


def transforms_to_rust(transforms, domain):
    out = []
    for selector, value in transforms.items():
        if isinstance(value, str):
            out.append(
                "(%s.into(), Transform::ToTag(%s.into()))"
                % (json_dumps(selector), json_dumps(value))
            )
        else:
            name = "%s::%s" % (domain, selector)
            out.append(
                "(%s.into(), Transform::Named(%s.into()))" % (json_dumps(selector), json_dumps(name))
            )
    return out


def gen_domain(obj, domain, parts, named):
    parts.append("        domain: %s.into()," % json_dumps(domain))
    sd = obj.get("supportedDomains", [])
    parts.append("        supported_domains: vec![%s]," % ", ".join("%s.into()" % json_dumps(x) for x in sd))

    for key in ("title", "author", "dek", "lead_image_url", "excerpt", "next_page_url"):
        if not obj.get(key):
            parts.append("        %s: None," % key)
            continue
        if key == "author" and isinstance(obj[key], str):
            parts.append("        author: Some(FieldValue::Value(%s.into()))," % json_dumps(obj[key]))
            continue
        items = field_to_rust(key, obj[key])
        if not items:
            parts.append("        %s: None," % key)
            continue
        if key == "author":
            parts.append(
                "        author: Some(FieldValue::Selectors(Field { selectors: vec!["
            )
            close = "        ]})),"
        else:
            parts.append("        %s: Some(Field { selectors: vec![" % key)
            close = "        ]}) ,"
        for item in items:
            parts.append("            %s," % item)
        parts.append(close)

    if obj.get("date_published"):
        dp = obj["date_published"]
        items = selectors_to_rust(dp.get("selectors", []), False)
        fmt = dp.get("format")
        tz = dp.get("timezone")
        parts.append("        date_published: Some(DateField {")
        parts.append("            selectors: vec![")
        for item in items:
            parts.append("                %s," % item)
        parts.append("            ],")
        parts.append(
            "            format: %s,"
            % (("Some(%s.into())" % json_dumps(fmt)) if fmt else "None")
        )
        parts.append(
            "            timezone: %s," % (("Some(%s.into())" % json_dumps(tz)) if tz else "None")
        )
        parts.append("        }),")
    else:
        parts.append("        date_published: None,")

    if obj.get("content"):
        content = obj["content"]
        items = selectors_to_rust(content.get("selectors", []), True)
        clean = content.get("clean", [])
        transforms = content.get("transforms", {}) or {}
        default_cleaner = content.get("defaultCleaner", True)
        parts.append("        content: Some(ContentField {")
        parts.append("            selectors: vec![")
        for item in items:
            parts.append("                %s," % item)
        parts.append("            ],")
        parts.append("            clean: vec![")
        for c in clean:
            if isinstance(c, list):
                for item in c:
                    parts.append("                %s.into()," % json_dumps(item))
            else:
                parts.append("                %s.into()," % json_dumps(c))
        parts.append("            ],")
        parts.append("            transforms: vec![")
        for t in transforms_to_rust(transforms, domain):
            parts.append("                %s," % t)
        parts.append("            ],")
        parts.append("            default_cleaner: %s," % ("true" if default_cleaner else "false"))
        parts.append("        }),")
        for selector, value in transforms.items():
            if not isinstance(value, str):
                named.append("%s::%s" % (domain, selector))
    else:
        parts.append("        content: None,")

    if obj.get("extend"):
        ext = obj["extend"]
        parts.append("        extend: HashMap::from([")
        for key, val in ext.items():
            allow = "true" if val.get("allowMultiple") else "false"
            sels = ", ".join("%s.into()" % json_dumps(x) for x in val.get("selectors", []))
            parts.append(
                "            (%s.into(), ExtendField { selectors: vec![%s], allow_multiple: %s }),"
                % (json_dumps(key), sels, allow)
            )
        parts.append("        ]),")
    else:
        parts.append("        extend: HashMap::new(),")


def generate(src_dir, out_path, named_out_path):
    domains = sorted(
        d
        for d in os.listdir(src_dir)
        if os.path.isdir(os.path.join(src_dir, d))
        and os.path.exists(os.path.join(src_dir, d, "index.js"))
    )
    entries = []
    named = []
    errors = []
    for domain in domains:
        src = open(os.path.join(src_dir, domain, "index.js"), encoding="utf-8").read()
        try:
            obj = parse_extractor(src, domain)
            parts = []
            gen_domain(obj, domain, parts, named)
            entries.append((domain, parts))
        except Exception as e:  # noqa: BLE001
            errors.append("%s: %s" % (domain, e))

    lines = []
    lines.append("// @generated by tools/generate_custom_extractors.py — do not edit by hand.")
    lines.append(
        "// Ported from upstream postlight/parser src/extractors/custom/ (MIT/Apache-2.0)."
    )
    lines.append("")
    lines.append("use std::collections::HashMap;")
    lines.append("")
    lines.append(
        "use crate::types::{ContentField, CustomExtractor, DateField, ExtendField, Field, FieldValue, Selector, Transform};"
    )
    lines.append("")
    lines.append("/// All built-in custom extractors, keyed by domain.")
    lines.append("pub fn all_extractors() -> Vec<CustomExtractor> {")
    lines.append("    vec![")
    for domain, parts in entries:
        lines.append("        CustomExtractor {")
        lines.extend(parts)
        lines.append("        },")
    lines.append("    ]")
    lines.append("}")
    lines.append("")

    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    with open(named_out_path, "w", encoding="utf-8") as f:
        for n in sorted(set(named)):
            f.write(n + "\n")

    print("generated %d extractors -> %s" % (len(entries), out_path))
    print("named transforms (%d) -> %s" % (len(set(named)), named_out_path))
    for e in sorted(set(named)):
        print("   ", e)
    if errors:
        print("ERRORS:")
        for e in errors:
            print("   ", e)
        sys.exit(1)


if __name__ == "__main__":
    generate(sys.argv[1], sys.argv[2], sys.argv[3])
