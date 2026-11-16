"""Small public-API witnesses for the frozen recovery-fence WASM source only."""
import hashlib
import json
from pathlib import Path

DIRECTORY = 'crates/oasis7_custody_worker/src/'
SOURCE_FILES = {
    'platform.rs': 'c8c096a487ccd4d9f5e8ae891eea4e4a97b739e44d29b9ccc0c45b9d0b50e13f',
    'recovery_fence_control.rs': '4a626ad5c407c5a04538dd83a1f056a56d54611a6f2acaa1153737f24ce1e861',
    'recovery_fence_store.rs': 'ac866482fc8d1d5439c2f6fc146350f70652a1d434616f7f7563181ccf1b3f79',
}
PACK = 'oasis7/rust-wasm-context'
NAME = 'WasmContext'
# Attribute expansion hides superseded Name/Body children in the public AST.
# Bind the original item by its frozen span, independently of a generated name.
MACROS = (
    ('event-fetch', 'Function', 'platform.rs', 15, 16, 'main', 'attribute', True),
    ('durable-object', 'Struct', 'platform.rs', 72, 74, 'RecoveryFence', 'attribute', True),
    ('wasm-export', 'Function', 'recovery_fence_control.rs', 55, 56, 'activate_recovery_fence', 'attribute', True),
    ('serde-derive', 'Struct', 'recovery_fence_store.rs', 172, 173, 'FenceDescription', 'derive', True),
    ('plain-function', 'Function', 'recovery_fence_control.rs', 18, 18, 'call', 'attribute', False),
    ('plain-struct', 'Struct', 'recovery_fence_store.rs', 166, 166, 'FenceStore', 'derive', False),
)
CALLS = (
    ('activation-call', 'recovery_fence_control.rs', 65, 'call', 'recovery_fence_control.rs', 18, 'bounded', 12),
    ('platform-control', 'platform.rs', 110, 'control_enabled', 'recovery_fence_store.rs', 40, 'policy', 28),
)


def verify_source(source):
    result = {}
    for name, expected in SOURCE_FILES.items():
        path = Path(source) / DIRECTORY / name
        if path.is_symlink() or not path.is_file():
            raise ValueError('witness source identity')
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != expected:
            raise ValueError('witness source identity')
        result[DIRECTORY + name] = {'bytes': len(data), 'sha256': expected}
    return result


def query(source):
    """One nine-row count query; no business strings or generated symbols returned."""
    source = Path(source)
    rows = []
    predicates = ['''import rust

private predicate atFile(AstNode n, string file) {
  n.getLocation().getFile().getAbsolutePath() = file
}

private predicate generatedBelow(AstNode n, MacroItems expansion) {
  n.getParentNode+() = expansion and n.isFromMacroExpansion()
}

private predicate target(Function f, string file, string name, int line) {
  atFile(f, file) and f.getName().getText() = name and
  f.getName().getLocation().getStartLine() = line
}

bindingset[c, key, value]
private predicate cfg(Crate c, string key, string value) {
  c.getACfgOption() = key + "=" + value or
  c.getACfgOption() = key + "=\\\"" + value + "\\\""
}

private predicate wasmRole(Crate c) {
  cfg(c, "target_arch", "wasm32") and cfg(c, "target_family", "wasm") and
  cfg(c, "target_os", "unknown") and cfg(c, "feature", "recovery-fence") and
  not cfg(c, "feature", ["identity", "owner", "executor", "approval-harness", "native-runtime-harness"])
}
''']
    for number, (label, cls, filename, start, line, name, kind, positive) in enumerate(MACROS):
        file = json.dumps(str(source / DIRECTORY / filename))
        expansion = 'getAttributeMacroExpansion' if kind == 'attribute' else 'getADeriveMacroExpansion'
        predicates.append(f'''
private predicate anchor{number}({cls} n) {{
  atFile(n, {file}) and not n.isFromMacroExpansion() and
  n.getLocation().getStartLine() = {start} and
  n.getLocation().getEndLine() >= {line} and
  // Public Name may be hidden only when an attribute macro supersedes it.
  (exists(n.getName()) implies
    (n.getName().getText() = {json.dumps(name)} and n.getName().getLocation().getStartLine() = {line}))
}}
''')
        rows.append(f'''label = {json.dumps(label)} and
    syntax = count({cls} n | anchor{number}(n)) and
    positive = count({cls} n, MacroItems m | anchor{number}(n) and m = n.{expansion}()) and
    negative = count({cls} n, AstNode child | anchor{number}(n) and generatedBelow(child, n.{expansion}()))''')
    for number, (label, filename, line, name, destination, declaration, wrong, wrong_line) in enumerate(CALLS):
        file = json.dumps(str(source / DIRECTORY / filename))
        dest = json.dumps(str(source / DIRECTORY / destination))
        predicates.append(f'''
private predicate call{number}(CallExpr c) {{
  atFile(c, {file}) and c.getLocation().getStartLine() = {line} and
  c.getFunction().(PathExpr).getPath().getText() = {json.dumps(name)}
}}
''')
        rows.append(f'''label = {json.dumps(label)} and
    syntax = count(CallExpr c | call{number}(c)) and
    positive = count(CallExpr c, Function f | call{number}(c) and f = c.(Call).getStaticTarget() and
      target(f, {dest}, {json.dumps(name)}, {declaration})) and
    negative = count(CallExpr c, Function f | call{number}(c) and f = c.(Call).getStaticTarget() and
      target(f, {dest}, {json.dumps(wrong)}, {wrong_line}))''')
    rows.append('''label = "wasm-role-cfg" and
    syntax = count(Crate c | c.getName() = "oasis7_custody_worker") and
    positive = count(Crate c | c.getName() = "oasis7_custody_worker" and wasmRole(c)) and
    negative = count(Crate c | c.getName() = "oasis7_custody_worker" and not wasmRole(c))''')
    return '\n'.join(predicates) + '\nfrom string label, int syntax, int positive, int negative\nwhere\n  (\n    ' + '\n    or\n    '.join(rows) + '\n  )\nselect label, syntax, positive, negative\n'


def validate(value):
    """Reject missing/duplicate/unknown rows; a zero negative alone proves nothing."""
    if (not isinstance(value, dict) or set(value) != {'#select'}
            or not isinstance(value['#select'], dict)
            or set(value['#select']) != {'columns', 'tuples'}):
        raise ValueError('witness result schema')
    table = value['#select']
    if table['columns'] != [{'name': 'label', 'kind': 'String'},
                            {'name': 'syntax', 'kind': 'Int'},
                            {'name': 'positive', 'kind': 'Int'},
                            {'name': 'negative', 'kind': 'Int'}]:
        raise ValueError('witness columns')
    expected = {row[0] for row in (*MACROS, *CALLS)} | {'wasm-role-cfg'}
    rows = table['tuples']
    if not isinstance(rows, list) or len(rows) != len(expected):
        raise ValueError('witness rows')
    counts = {}
    for row in rows:
        if (not isinstance(row, list) or len(row) != 4 or not isinstance(row[0], str)
                or row[0] not in expected or row[0] in counts
                or any(type(n) is not int or not 0 <= n <= 2**31-1 for n in row[1:])):
            raise ValueError('witness row')
        counts[row[0]] = row[1:]
    checks = {}
    for label, *_rest, positive in MACROS:
        syntax, expansions, generated = counts[label]
        checks[label] = syntax > 0 and (expansions > 0 and generated > 0 if positive else expansions == generated == 0)
    for label, *_rest in CALLS:
        syntax, correct, wrong = counts[label]
        checks[label] = syntax > 0 and correct > 0 and wrong == 0
    syntax, correct, wrong = counts['wasm-role-cfg']
    checks['wasm-role-cfg'] = syntax > 0 and correct == syntax and wrong == 0
    return {'status': 'PASS' if all(checks.values()) else 'INCOMPLETE', 'checks': checks, 'counts': counts,
            'macros': 'PASS' if all(checks[r[0]] for r in MACROS) else 'INCOMPLETE',
            'static_callees': 'PASS' if all(checks[r[0]] for r in CALLS) else 'INCOMPLETE',
            'wasm_role_cfg': 'PASS' if checks['wasm-role-cfg'] else 'INCOMPLETE',
            'scope': 'four real expansions and two static callees, with negatives; no general dataflow or full WASM coverage'}
