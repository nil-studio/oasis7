"""Pure-stdlib Rust lexical scanner, module/include extractor, and three-valued cfg evaluator."""

from __future__ import annotations
import ast
import re
import unicodedata

_OPEN_TO_CLOSE = {"(": ")", "[": "]", "{": "}"}
_CLOSE_TO_OPEN = {v: k for k, v in _OPEN_TO_CLOSE.items()}
_IDENT_RE = re.compile(r"(?:r#)?[A-Za-z_][A-Za-z0-9_]*")


def _unquote_rust_string(token: str) -> str | None:
    if token.startswith(('b"', 'c"', 'br"', 'br#', 'cr"', 'cr#')):
        token = token[1:]
    if token.startswith("r"):
        idx = 1
        while idx < len(token) and token[idx] == "#":
            idx += 1
        if idx < len(token) and token[idx] == '"' and token.endswith('"' + "#" * (idx - 1)):
            return token[idx + 1 : len(token) - idx]
        return None
    if len(token) >= 2 and token[0] == '"' and token[-1] == '"':
        try:
            return ast.literal_eval(token)
        except (SyntaxError, ValueError):
            return token[1:-1]
    return None


def tokenize_rust_source(text: str) -> tuple[list[tuple[str, str, int]], dict[int, int]]:
    """Tokenize Rust source into (kind, value, line) tuples and matching delimiter map."""
    tokens: list[tuple[str, str, int]] = []
    closing: dict[int, int] = {}
    stack: list[tuple[str, int]] = []
    i, n, line = 0, len(text), 1
    while i < n:
        ch = text[i]
        if ch in " \t\r\n":
            line += ch == "\n"
            i += 1
            continue
        if ch == "/" and i + 1 < n and text[i + 1] == "/":
            nl = text.find("\n", i + 2)
            i = n if nl == -1 else nl
            continue
        if ch == "/" and i + 1 < n and text[i + 1] == "*":
            depth, i = 1, i + 2
            while i < n and depth > 0:
                if text[i] == "\n":
                    line += 1
                elif text[i : i + 2] == "/*":
                    depth += 1
                    i += 1
                elif text[i : i + 2] == "*/":
                    depth -= 1
                    i += 1
                i += 1
            continue
        raw_p = 1 if (ch == "r" and i + 1 < n and text[i + 1] in '"#') else (
            2 if (ch in "bc" and i + 2 < n and text[i + 1] == "r" and text[i + 2] in '"#') else 0)
        if raw_p:
            j = i + raw_p
            while j < n and text[j] == "#":
                j += 1
            if j < n and text[j] == '"':
                end_marker = '"' + "#" * (j - i - raw_p)
                end = text.find(end_marker, j + 1)
                raw_val = text[i:] if end == -1 else text[i : end + len(end_marker)]
                tokens.append(("string", raw_val, line))
                line += raw_val.count("\n")
                i = n if end == -1 else end + len(end_marker)
                continue
        if ch == '"' or (ch in "bc" and i + 1 < n and text[i + 1] == '"'):
            start, start_line, i = i, line, i + (1 if ch == '"' else 2)
            while i < n and text[i] != '"':
                if text[i] == "\\":
                    i += 1
                line += (i < n and text[i] == "\n")
                i += 1
            i = min(n, i + 1)
            tokens.append(("string", text[start:i], start_line))
            continue
        if ch == "'":
            if i + 2 < n and text[i + 1] == "\\" and text[i + 2] != "\n":
                end = text.find("'", i + 3)
                if end != -1 and end - i <= 12:
                    tokens.append(("char", text[i : end + 1], line))
                    i = end + 1
                    continue
            if i + 2 < n and text[i + 2] == "'" and text[i + 1] != "\n":
                tokens.append(("char", text[i : i + 3], line))
                i += 3
                continue
            m = _IDENT_RE.match(text, i + 1)
            if m:
                tokens.append(("lifetime", text[i : m.end()], line))
                i = m.end()
                continue
        m = _IDENT_RE.match(text, i)
        if m:
            tokens.append(("identifier", m.group(0), line))
            i = m.end()
            continue
        if ch in _OPEN_TO_CLOSE:
            stack.append((ch, len(tokens)))
            tokens.append(("delim", ch, line))
            i += 1
            continue
        if ch in _CLOSE_TO_OPEN:
            idx = len(tokens)
            tokens.append(("delim", ch, line))
            if stack and stack[-1][0] == _CLOSE_TO_OPEN[ch]:
                closing[stack.pop()[1]] = idx
            i += 1
            continue
        if text[i : i + 2] == "::":
            tokens.append(("punct", "::", line))
            i += 2
            continue
        tokens.append(("punct", ch, line))
        i += 1
    return tokens, closing


def _extract_out_dir_concat(inner: list[tuple[str, str, int]]) -> str | None:
    if len(inner) < 3 or inner[0][1] != "concat" or inner[1][1] != "!" or inner[2][1] not in ("(", "[", "{"):
        return None
    parts = [v for _, v, _ in inner[3:-1]]
    if "env" not in parts or '"OUT_DIR"' not in parts:
        return None
    literals = [_unquote_rust_string(v) for k, v, _ in inner[3:-1] if k == "string"]
    out_idx = next((idx for idx, val in enumerate(literals) if val == "OUT_DIR"), -1)
    suffix = [p for p in literals[out_idx + 1 :] if p is not None] if out_idx >= 0 else []
    return "".join(suffix) if suffix else None


def extract_source_inputs(text: str) -> tuple[list[dict], list[dict], bool]:
    """Extract mod declarations, include*! calls, and uncertain #[path] flags from Rust source."""
    tokens, closing = tokenize_rust_source(text)
    mods, includes, uncertain_paths, pending_path, i, n = [], [], False, None, 0, len(tokens)
    while i < n:
        kind, val, line = tokens[i]
        if val == "#" and i + 1 < n:
            open_idx = i + (2 if tokens[i + 1][1] == "!" else 1)
            if open_idx in closing and tokens[open_idx][1] == "[":
                end_idx = closing[open_idx]
                attr = tokens[open_idx + 1 : end_idx]
                if attr:
                    head = attr[0][1].removeprefix("r#")
                    if head == "path":
                        lit = (_unquote_rust_string(attr[2][1])
                               if len(attr) == 3 and attr[1][1] == "=" and attr[2][0] == "string" else None)
                        if lit is None:
                            uncertain_paths = True
                        else:
                            pending_path = lit
                    elif head == "cfg_attr" and any(t[0] == "identifier" and t[1].removeprefix("r#") == "path" for t in attr[1:]):
                        uncertain_paths = True
                i = end_idx + 1
                continue
        if kind == "identifier" and val == "mod" and i + 1 < n and tokens[i + 1][0] == "identifier":
            mod_name = tokens[i + 1][1].removeprefix("r#")
            nxt = tokens[i + 2][1] if i + 2 < n else ""
            if nxt in (";", "{"):
                if nxt == "{" and pending_path is not None:
                    uncertain_paths = True
                mods.append({"name": mod_name, "path": pending_path, "inline": nxt == "{", "line": line})
            pending_path = None
            i += 2
            continue
        if kind == "identifier" and val in ("include", "include_bytes", "include_str") and i + 2 < n and tokens[i + 1][1] == "!" and (i + 2) in closing:
            end_idx = closing[i + 2]
            inner = tokens[i + 3 : end_idx]
            lit = _unquote_rust_string(inner[0][1]) if len(inner) == 1 and inner[0][0] == "string" else None
            gen = None if lit is not None else _extract_out_dir_concat(inner)
            if lit is not None:
                includes.append({"macro": val, "kind": "literal", "path": lit, "line": line})
            elif gen is not None:
                includes.append({"macro": val, "kind": "generated", "path": gen, "line": line})
            else:
                includes.append({"macro": val, "kind": "unknown", "path": None, "line": line})
            i = end_idx + 1
            continue
        if val in (";", "{", "}"):
            pending_path = None
        i += 1
    return mods, includes, uncertain_paths


def _split_top_level_commas(tokens: list[tuple[str, str, int]]) -> list[list[tuple[str, str, int]]]:
    parts, cur, depth = [], [], 0
    for tok in tokens:
        depth += (tok[1] in "([{") - (tok[1] in ")]}")
        if tok[1] == "," and depth == 0:
            if cur:
                parts.append(cur)
                cur = []
            continue
        cur.append(tok)
    if cur:
        parts.append(cur)
    return parts


def eval_cfg_expr(
    expr_tokens: list[tuple[str, str, int]], *, target_archs: frozenset[str] = frozenset(),
    features: frozenset[str] = frozenset(), test: bool | None = None, extra_flags: frozenset[str] = frozenset(),
) -> bool | None:
    """Three-valued (True/False/None) evaluator for Rust cfg(...) predicate tokens."""
    if not expr_tokens:
        return None
    if len(expr_tokens) == 1 and expr_tokens[0][0] == "identifier":
        name = expr_tokens[0][1].removeprefix("r#")
        return test if name == "test" else (True if name in extra_flags else None)
    if len(expr_tokens) == 3 and expr_tokens[1][1] == "=" and expr_tokens[2][0] == "string":
        key, val = expr_tokens[0][1].removeprefix("r#"), _unquote_rust_string(expr_tokens[2][1])
        if val is None:
            return None
        if key == "feature":
            return val in features
        return (val in target_archs) if (key == "target_arch" and target_archs) else None
    if len(expr_tokens) >= 3 and expr_tokens[0][0] == "identifier" and expr_tokens[1][1] == "(" and expr_tokens[-1][1] == ")":
        op = expr_tokens[0][1].removeprefix("r#")
        vals = [eval_cfg_expr(a, target_archs=target_archs, features=features, test=test, extra_flags=extra_flags)
                for a in _split_top_level_commas(expr_tokens[2:-1])]
        if op == "not":
            return (not vals[0]) if len(vals) == 1 and vals[0] is not None else None
        if op == "all":
            return False if any(v is False for v in vals) else (True if all(v is True for v in vals) else None)
        if op == "any":
            return True if any(v is True for v in vals) else (False if all(v is False for v in vals) else None)
    return None


def classify_item_bodies(tokens: list[tuple[str, str, int]], closing: dict[int, int]) -> list[dict[str, object]]:
    """Classify struct/enum type_body and fn body token spans."""
    spans: list[dict[str, object]] = []
    i, n = 0, len(tokens)
    while i < n:
        kind, val, line = tokens[i]
        if kind == "identifier" and val in ("struct", "enum", "fn") and i + 1 < n and tokens[i + 1][0] == "identifier":
            j = i + 2
            while j < n and tokens[j][1] not in ("{", ";"):
                j = (closing[j] + 1) if (j in closing and tokens[j][1] in "([") else (j + 1)
            if j < n and tokens[j][1] == "{" and j in closing:
                spans.append({"kind": "body" if val == "fn" else "type_body", "item": val,
                              "name": tokens[i + 1][1].removeprefix("r#"), "start": j, "end": closing[j], "line": line})
                i = j + 1
                continue
        i += 1
    return spans


_WS_RE, _BSTEP_RE, _RAW_RE = re.compile(r"\s+"), re.compile(r"/\*|\*/"), re.compile(r'(?:br|cr|r)(#*)"')
_SBODY_RE, _FIDENT_RE = re.compile(r'(?:[^"\\]|\\.)*"', re.DOTALL), re.compile(r"(?:r#)?(?:[^\W\d]|_)\w*")
_SUF_RE, _NUM_RE = re.compile(r"(?:[^\W\d]|_)\w*"), re.compile(r"[0-9][0-9_]*(?:\.[0-9_]+)?(?:[eE][+-]?[0-9_]+)?")
_CHR_RE = re.compile(r"(?:b)?'(?:[^'\\\r\n]|\\(?:[nrt0\\'\"]|x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}))'")
_PSTR_RE, _PARG_RE = re.compile(r'(?:b|c)?"([^"\\\r\n]*)"'), re.compile(r'"([^"\\\r\n]*)"')
_CENV_RE = re.compile(r'concat!\(env!\("(CARGO_MANIFEST_DIR|OUT_DIR)",?\),"([^"\\\r\n]+)",?\)')


def rust_literal_path(v: str) -> str | None:
    p, r = re.fullmatch(r'"([^"\\]+)"', v), re.fullmatch(r'r(#+)"(.*)"\1', v)
    return v[2:-1] if (v.startswith('r"') and v.endswith('"')) else (p.group(1) if p else r.group(2) if r else None)


def scan_rust_source_inputs(source: str, return_tokens: bool = False, with_module_paths: bool = False, with_module_safety: bool = False):
    tokens: list[tuple[str, str, int]] = []
    pos, slen = 0, len(source)

    def add_lit(kind: str, s: int, e: int) -> int:
        if e < slen and (source[e] == "_" or source[e].isalpha() or not source[e].isascii()) and (m := _SUF_RE.match(source, e)):
            e = m.end()
            if e < slen and not source[e].isascii():
                while e < slen and ("_" + source[e]).isidentifier():
                    e += 1
            kind = "suffixed_literal"
        tokens.append((kind, source[s:e], s))
        return e

    while pos < slen:
        if ws := _WS_RE.match(source, pos):
            pos = ws.end()
            if pos >= slen:
                break
        start, ch = pos, source[pos]
        if ch == "/" and source.startswith("//", pos):
            nl = source.find("\n", pos + 2)
            pos = slen if nl < 0 else nl + 1
        elif ch == "/" and source.startswith("/*", pos):
            depth, pos = 1, pos + 2
            while depth:
                if (step := _BSTEP_RE.search(source, pos)) is None:
                    raise ValueError("unterminated Rust block comment")
                depth, pos = depth + (1 if step.group(0) == "/*" else -1), step.end()
        elif ch in ("r", "b", "c") and (raw := _RAW_RE.match(source, pos)):
            cd = '"' + raw.group(1)
            if (end := source.find(cd, raw.end())) < 0:
                raise ValueError("unterminated Rust raw string")
            pos = add_lit("string", start, end + len(cd))
        elif ch == '"' or (ch in ("b", "c") and pos + 1 < slen and source[pos + 1] == '"'):
            if (body := _SBODY_RE.match(source, (pos if ch == '"' else pos + 1) + 1)) is None:
                raise ValueError("unterminated Rust string")
            pos = add_lit("string", start, body.end())
        elif (ch == "'" or (ch == "b" and pos + 1 < slen and source[pos + 1] == "'")) and (cm := _CHR_RE.match(source, pos)):
            pos = add_lit("character", start, cm.end())
        elif "0" <= ch <= "9":
            pos = add_lit("number", start, _NUM_RE.match(source, pos).end())
        elif (ch == "_" or ("a" <= ch <= "z") or ("A" <= ch <= "Z") or not ch.isascii()) and (im := _FIDENT_RE.match(source, pos)):
            pos = im.end()
            if pos < slen and not source[pos].isascii():
                while pos < slen and ("_" + source[pos]).isidentifier():
                    pos += 1
            tokens.append(("identifier", source[start:pos], start))
        else:
            tokens.append(("punctuation", ch, pos)); pos += 1

    closing, stack, pairs = {}, [], {")": "(", "]": "[", "}": "{"}
    for idx, (kind, val, _) in enumerate(tokens):
        if kind == "punctuation" and val in ("(", "[", "{"):
            stack.append(idx)
        elif kind == "punctuation" and val in pairs:
            if not stack or tokens[stack[-1]][1] != pairs[val]:
                raise ValueError("unbalanced Rust token delimiters")
            closing[stack.pop()] = idx
    if stack:
        raise ValueError("unclosed Rust token delimiter")
    if return_tokens:
        return tokens, closing

    literals, lit_tokens, calls = [], [], []
    for idx, (kind, val, start) in enumerate(tokens):
        if kind == "string":
            prev = len(literals)
            if lm := _PSTR_RE.fullmatch(val):
                literals.append(lm.group(1))
            elif rm := _RAW_RE.match(val):
                literals.append(val[rm.end() : -(len(rm.group(1)) + 1)])
            if with_module_safety and len(literals) != prev:
                lit_tokens.append((idx, literals[-1]))
        name = val.removeprefix("r#")
        if kind != "identifier" or name not in ("include", "include_str", "include_bytes") or idx + 1 >= len(tokens) or tokens[idx + 1][1] != "!":
            continue
        end, cur, ns = closing.get(idx + 2), idx - 1, []
        call = {"macro": name, "kind": "unknown", "path": None, "line": source.count("\n", 0, start) + 1,
                "expression": source[start : tokens[end][2] + 1] if end is not None else val + "!"}
        while cur > 0 and tokens[cur - 1][1] == tokens[cur][1] == ":" and cur - 2 >= 0 and tokens[cur - 2][0] == "identifier":
            ns.append(tokens[cur - 2][1].removeprefix("r#")); cur -= 3
        if cur > 0 and tokens[cur - 1][1] == tokens[cur][1] == ":":
            cur -= 2
        qual = ((idx > 1 and tokens[idx - 2][1] == tokens[idx - 1][1] == ":" and ns not in (["std"], ["core"])) or (cur >= 0 and tokens[cur][1] == "$"))
        if end is not None and not qual:
            args = tokens[idx + 3 : end]
            args = args[:-1] if (args and args[-1][1] == ",") else args
            expr = "".join(t[1] for t in args)
            if len(args) == 1 and (pm := _PARG_RE.fullmatch(expr)):
                call.update(kind="literal", path=pm.group(1))
            elif cm := _CENV_RE.fullmatch(expr):
                call.update(kind="manifest" if cm.group(1) == "CARGO_MANIFEST_DIR" else "generated", path=cm.group(2))
        calls.append(call)
    if not with_module_paths:
        return literals, calls
    uncertain = False
    for idx, (_, tok, _) in enumerate(tokens[:-1]):
        if tok == "#" and idx + 1 in closing and tokens[idx + 1][1] == "[":
            vals = tuple(v.removeprefix("r#") if k == "identifier" else v for k, v, _ in tokens[idx + 2 : closing[idx + 1]])
            cur = closing[idx + 1] + 1
            while cur < len(tokens) and tokens[cur][1] not in ("mod", "{", "}", ";"):
                cur = closing[cur + 1] + 1 if (tokens[cur][1] == "#" and cur + 1 in closing) else cur + 1
            inl = bool(vals) and vals[0] == "path" and cur + 2 < len(tokens) and tokens[cur][1] == "mod" and tokens[cur + 2][1] == "{"
            uncertain |= bool(vals) and ((vals[0] == "path" and (len(vals) != 3 or vals[1] != "=" or rust_literal_path(vals[2]) is None)) or (vals[0] == "cfg_attr" and "path" in vals) or inl)
    if not with_module_safety:
        return literals, calls, uncertain
    m_ranges = [(s, e) for s, e in closing.items() if (s and tokens[s - 1][1] == "!") or (s > 1 and tokens[s - 2][1] == "!" and tokens[s - 1][0] == "identifier")]
    opaque = any(idx + 2 >= len(tokens) or tokens[idx + 1][0] != "identifier" or tokens[idx + 2][1] not in (";", "{") or any(s < idx < e for s, e in m_ranges) for idx, (k, t, _) in enumerate(tokens) if k == "identifier" and t == "mod")
    inc_set = {"include", "include_str", "include_bytes"}
    for idx, (kind, tok, _) in enumerate(tokens):
        if tok == "use":
            eu = next((j for j in range(idx + 1, len(tokens)) if tokens[j][1] == ";"), len(tokens))
            opaque |= any(v.removeprefix("r#") in inc_set for k, v, _ in tokens[idx + 1 : eu] if k == "identifier")
        opaque |= (kind == "identifier" and tok.removeprefix("r#") in inc_set and (idx + 1 >= len(tokens) or tokens[idx + 1][1] != "!") and any(s < idx < e for s, e in m_ranges)) or (tok == "$" and idx + 2 < len(tokens) and tokens[idx + 1][0] == "identifier" and tokens[idx + 2][1] == "!") or (kind == "identifier" and (tok == "macro" or (not tok.isascii() and unicodedata.normalize("NFC", tok) != tok)))
    data_lits = [lv for idx, lv in lit_tokens if not (idx >= 4 and idx + 1 < len(tokens) and [v for _, v, _ in tokens[idx - 4 : idx]] == ["#", "[", "path", "="] and tokens[idx + 1][1] == "]") and not (idx >= 3 and idx + 1 < len(tokens) and [v for _, v, _ in tokens[idx - 3 : idx]] == ["include", "!", "("] and tokens[idx + 1][1] in (",", ")"))]
    return literals, calls, uncertain, opaque, tuple(data_lits)
