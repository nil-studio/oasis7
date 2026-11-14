#!/usr/bin/env python3
"""Private 37+7 candidate workload, sharing the extraction's fixed analysis window."""
import argparse
from contextlib import ExitStack
import csv
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import time

from codeql_rust_action_deadline import shared_budget
import codeql_rust_candidate_profile as candidate_profile

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('workload_comparison', HERE / 'codeql-rust-qualifier-comparison.py')
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)
capacity, runtime = comparison.capacity, comparison.runtime

MODE = 'candidate-workload-research'
# Keep the v3 and v5 intermediate identities distinct from the final v6 library.
VARIANT = 'qualifier-lines-v3'
V5_VARIANT = 'qualifier-lines-shared-trait-v5'
WORKLOAD_VARIANT = 'qualifier-lines-constraint-matching-v6'
SHARED_FILE = capacity.bundle.SHARED_FILE
SHARED_OFFICIAL_SHA = capacity.bundle.SHARED_OFFICIAL_SHA
SHARED_CANDIDATE_SHA = capacity.bundle.SHARED_CANDIDATE_SHA
SHARED_PATCH_SHA = capacity.bundle.SHARED_PATCH_SHA
SHARED_UPSTREAM = capacity.bundle.SHARED_UPSTREAM
CONSTRAINT_UPSTREAM = '1c4fea90af949111bc1c625f1e72de56a12c7712'
CONSTRAINT_API_PATCH_SHA = 'ba47e346672cd8281b3a6a3d94647b0d00d733a273311799b0533dc4b0fd37db'
CONSTRAINT_UPSTREAM_PATCH_SHA = '3880e57052c74d8af7f155a7abc8b721eef0725463897647809fd10b28d3a0ba'
CONSTRAINT_PATCH_SHA = 'e6563cf895df0f662444795cf5c8f2358804784d667ccce4b11fa336ffde7db8'
CONSTRAINT_CANDIDATE_SHA = '8b32c4d95cfd39f0fef5d365e27b3a0d734ae815272dc714fc60d40de56edc33'
ARITY_PATCH_SHA = '89a3e6fb117edb4de7f89f2fbc135da98618ddd73aabcd0b72df164e260354c3'
TYPE_CANDIDATE_SHA = 'd502d88bafabca4c3b41e173c362ce80660bd8c096ee95e53a47f124deaf33fb'
TRAIT_PATCH_SHA = '15d67391f614b0ef8ef5cc6ff4a2b404bb98b4656b33ff41a4f6292db8ec1c21'
TRAIT_TYPE_SHA = 'db190947441166d6a24217f4dd3bdeafdf8965ca4e11998eba54080bbf91016b'
TRAIT_PATH_FILE = 'codeql/rust/internal/PathResolution.qll'
TRAIT_PATH_OFFICIAL_SHA = 'c83ebb64d9a10628ec37afaac3543ad0e2ac36f1297cffd4e85e29ea66f9fa8d'
TRAIT_PATH_SHA = '222a5a43252a4055a66b38d70b2177668e3ebdbd918de67eeb7e9c206cc9ebbf'
IMAGE_CONTRACT = capacity.IMAGE_V2
MODEL_FILE = 'codeql/rust/frameworks/stdlib/core.model.yml'
MODEL_OFFICIAL_SHA = '756cb0231b80f70c944cef16ed8c7db449580629b70328ec1e776d4c3f206400'
MODEL_CANDIDATE_SHA = '4a2534bf0f566206409fa9a9f24eca3b2744c8f79b33409689f680bb4f504734'
MODEL_MARKER = b'      # Str\n'
MODEL_LINE = b'      - ["<core::str>::lines", "Argument[self].Reference", "ReturnValue.Element.Reference", "taint", "manual"]\n'
DEFAULT_PACK = 'oasis7/rust-default-candidate-research'
WITNESS_PACK = 'oasis7/rust-public-candidate-research'
DEFAULT_ENTRIES_SHA = '53422ffb03812452686f24b03ee61ebf3beabab3cceb09a733947877e4d777c6'
SUITE_SHA = '97f06744235dc677060b14a4ca77d26d2a170b855f054a4672aa732321c86605'
PRIVATE_CAP = 64 * 1024**2
TAIL_SECONDS, EVIDENCE_SECONDS, REQUIRED_MARGIN = 600, 120, 270
WITNESS_COLUMNS = {
    'PeerCandidate': ['label', 'present'], 'PeerLegacy': ['label', 'present'],
    'Driver': ['label', 'present'], 'Image': ['label', 'present'],
    'Files': ['col0', 'col1'],
    'CallsCandidate': ['col0', 'col1', 'col2', 'col3'],
    'CallsLegacy': ['col0', 'col1', 'col2', 'col3'],
}


def witness_rows(name, path):
    with path.open() as stream:
        rows = list(csv.reader(stream))
    # The frozen metric queries select named variables; the Files and Calls
    # queries select unnamed expressions. Do not infer schema from row values.
    if not rows or rows[0] != WITNESS_COLUMNS.get(name):
        raise RuntimeError('public witness decode schema differs: ' + name)
    return rows[1:]


def read_json(path, limit=16 * 1024**2):
    if path.resolve() != path or not path.is_file() or path.stat().st_size > limit:
        raise RuntimeError('missing, unsafe or oversized JSON: ' + str(path))
    return json.loads(path.read_text())


def regular_snapshot(root, end):
    if root.resolve() != root or not root.is_dir():
        raise RuntimeError('unsafe private input root')
    paths = list(root.rglob('*'))
    if len(paths) > 8192 or any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in paths):
        raise RuntimeError('unsafe private input inventory')
    return capacity.official_snapshot(root, end=end)


def private_inputs(work, end):
    return {name: regular_snapshot(work / name, end) for name in
            ('candidate-rust-all', 'candidate-typeinference', 'default-queries', 'public-witnesses')}


def workload_closure(official, work):
    closure = comparison.candidate_closure(official, work / 'candidate-rust-all')
    closure['codeql/typeinference']['root'] = str(work / 'candidate-typeinference')
    return closure


def shared_helper_identity(original, private, end):
    identity = capacity.bundle.shared_helper_identity(original, private,
                                                      snapshot=lambda root: regular_snapshot(root, end))
    return dict(variant=V5_VARIANT, base_variant=VARIANT, **identity)


def prepare_shared_helper(closure, work, out, end, run, *, copy_end):
    def copy_file(source, target):
        capacity.check_time(copy_end)
        return shutil.copy2(source, target)
    identity = capacity.bundle.prepare_shared_helper(closure['codeql/typeinference'], work / 'candidate-typeinference',
        HERE / 'codeql-rust-shared-constraint.patch', snapshot=lambda root: regular_snapshot(root, end),
        copy_file=copy_file, file_digest=lambda path: capacity.digest_until(path, end), run=run)
    runtime.save(out / 'shared-helper-identity.json', dict(variant=V5_VARIANT, base_variant=VARIANT, **identity), out)


def verify_v5_identity(closure, work, out, end):
    """Validate the genuine v5 tree before applying the independent v6 overlay."""
    if trait_visibility_identity(Path(closure['codeql/rust-all']['root']),
                                 work / 'candidate-rust-all', out, end) != read_json(out / 'trait-visibility-identity.json'):
        raise RuntimeError('trait visibility identity differs after preparation')
    if shared_helper_identity(Path(closure['codeql/typeinference']['root']),
                              work / 'candidate-typeinference', end) != read_json(out / 'shared-helper-identity.json'):
        raise RuntimeError('shared helper identity differs after preparation')


def constraint_matching_identity(closure, work, out, end):
    """Revalidate the saved v5 shared base before checking the actual final tree."""
    if trait_visibility_identity(Path(closure['codeql/rust-all']['root']),
                                 work / 'candidate-rust-all', out, end) != read_json(out / 'trait-visibility-identity.json'):
        raise RuntimeError('trait visibility identity differs after preparation')
    base = read_json(out / 'shared-helper-identity.json')
    before = base['candidate_files']
    original = regular_snapshot(Path(closure['codeql/typeinference']['root']), end)
    expected = dict(variant=V5_VARIANT, base_variant=VARIANT,
                    **capacity.bundle.shared_snapshot_identity(original, before))
    if expected != base:
        raise RuntimeError('v5 shared intermediate identity differs after preparation')
    after = regular_snapshot(work / 'candidate-typeinference', end)
    changed = sorted(p for p in set(before) | set(after) if before.get(p) != after.get(p))
    if (runtime.digest(HERE / 'codeql-rust-constraint-matching.patch') != CONSTRAINT_PATCH_SHA
            or set(before) != set(after) or changed != [SHARED_FILE]
            or after[SHARED_FILE]['sha256'] != CONSTRAINT_CANDIDATE_SHA
            or any(before[p]['mode'] != after[p]['mode'] for p in before)):
        raise RuntimeError('candidate differs from the fixed ConstraintMatching overlay')
    return dict(variant=WORKLOAD_VARIANT, base_variant=V5_VARIANT,
                upstream_commit=CONSTRAINT_UPSTREAM, upstream_api_patch_sha256=CONSTRAINT_API_PATCH_SHA,
                upstream_patch_sha256=CONSTRAINT_UPSTREAM_PATCH_SHA, patch_sha256=CONSTRAINT_PATCH_SHA,
                base_receipts_sha256={name: runtime.digest(out / name) for name in
                    ('shared-helper-identity.json', 'trait-visibility-identity.json')},
                changed_files=changed, before_files=before, candidate_files=after)


def verify_workload_identity(closure, work, out, end):
    if constraint_matching_identity(closure, work, out, end) != read_json(out / 'constraint-matching-identity.json'):
        raise RuntimeError('ConstraintMatching identity differs after preparation')


def prepare_constraint_matching(closure, work, out, end, run):
    verify_v5_identity(closure, work, out, end)
    patch_file = HERE / 'codeql-rust-constraint-matching.patch'
    if patch_file.resolve() != patch_file or runtime.digest(patch_file) != CONSTRAINT_PATCH_SHA:
        raise RuntimeError('ConstraintMatching patch identity differs')
    run(['patch', '--batch', '--forward', '--no-backup-if-mismatch', '--fuzz=0', '-p1',
         '-d', work / 'candidate-typeinference', '-i', patch_file], 'apply-constraint-matching-patch', 20)
    runtime.save(out / 'constraint-matching-identity.json',
                 constraint_matching_identity(closure, work, out, end), out)


def arity_identity():
    return dict(variant=VARIANT, patch_sha256=ARITY_PATCH_SHA,
                base_patch_sha256=comparison.PATCH_SHA, base_type_sha256=comparison.CANDIDATE_SHA,
                candidate_type_sha256=TYPE_CANDIDATE_SHA, changed_files=[comparison.TYPE_FILE])


def prepare_candidate(closure, work, out, end, run, *, copy_end):
    """Preserve the legacy qualifier patch, then bind the v3 arity and lines changes."""
    private = comparison.prepare_candidate(closure, work, out, end, run, copy_end=copy_end)
    patch_file = HERE / 'codeql-rust-qualifier-arity.patch'
    type_file = private / comparison.TYPE_FILE
    if runtime.digest(patch_file) != ARITY_PATCH_SHA or runtime.digest(type_file) != comparison.CANDIDATE_SHA:
        raise RuntimeError('arity patch or base qualifier identity differs')
    before = regular_snapshot(private, end)
    original = Path(closure['codeql/rust-all']['root'])
    official = regular_snapshot(original, end)
    changed = sorted(p for p in set(official) | set(before) if official.get(p) != before.get(p))
    if (set(official) != set(before) or changed != [comparison.TYPE_FILE]
            or official[comparison.TYPE_FILE]['mode'] != before[comparison.TYPE_FILE]['mode']):
        raise RuntimeError('candidate differs from the fixed two-file model before the arity patch')
    lines_model_bytes(private / MODEL_FILE, end)
    run(['patch', '--batch', '--fuzz=0', type_file, patch_file], 'apply-arity-patch', 20)
    after = regular_snapshot(private, end)
    changed = sorted(p for p in set(before) | set(after) if before.get(p) != after.get(p))
    if (set(before) != set(after) or changed != [comparison.TYPE_FILE]
            or before[comparison.TYPE_FILE]['mode'] != after[comparison.TYPE_FILE]['mode']
            or runtime.digest(type_file) != TYPE_CANDIDATE_SHA):
        raise RuntimeError('candidate differs beyond the fixed arity patch')
    runtime.save(out / 'arity-patch-identity.json', arity_identity(), out)
    prepare_lines_model(original, private, out, end)
    return private


def verify_candidate_identity(original, private, out, end):
    if read_json(out / 'arity-patch-identity.json') != arity_identity():
        raise RuntimeError('candidate arity patch identity differs after preparation')
    if lines_identity(original, private, end) != read_json(out / 'lines-model-identity.json'):
        raise RuntimeError('candidate two-file model identity differs after preparation')


def lines_identity(original, private, end):
    return lines_snapshot_identity(regular_snapshot(original, end), regular_snapshot(private, end))


def lines_snapshot_identity(official_files, candidate_files):
    """Validate either the current v3 tree or its frozen intermediate receipt."""
    changed = sorted(p for p in set(official_files) | set(candidate_files)
                     if official_files.get(p) != candidate_files.get(p))
    if (set(official_files) != set(candidate_files) or changed != sorted((comparison.TYPE_FILE, MODEL_FILE))
            or official_files[comparison.TYPE_FILE]['sha256'] != comparison.OFFICIAL_SHA
            or candidate_files[comparison.TYPE_FILE]['sha256'] != TYPE_CANDIDATE_SHA
            or official_files[MODEL_FILE]['sha256'] != MODEL_OFFICIAL_SHA
            or candidate_files[MODEL_FILE]['sha256'] != MODEL_CANDIDATE_SHA
            or any(official_files[p]['mode'] != candidate_files[p]['mode'] for p in changed)):
        raise RuntimeError('candidate differs from the fixed qualifier/lines two-file model')
    return dict(variant=VARIANT, image_contract_version=IMAGE_CONTRACT,
                summary=MODEL_LINE.decode().strip(), changed_files=changed,
                official_files=official_files, candidate_files=candidate_files)


def trait_visibility_identity(original, private, out, end):
    """Bind the annotated Rust tree to the genuine, unchanged v3 intermediate."""
    if read_json(out / 'arity-patch-identity.json') != arity_identity():
        raise RuntimeError('candidate arity patch identity differs after preparation')
    base = read_json(out / 'lines-model-identity.json')
    before = base['candidate_files']
    if lines_snapshot_identity(regular_snapshot(original, end), before) != base:
        raise RuntimeError('candidate v3 intermediate identity differs after preparation')
    after = regular_snapshot(private, end)
    changed = sorted(p for p in set(before) | set(after) if before.get(p) != after.get(p))
    if (runtime.digest(HERE / 'codeql-rust-trait-visibility.patch') != TRAIT_PATCH_SHA
            or set(before) != set(after) or changed != sorted((comparison.TYPE_FILE, TRAIT_PATH_FILE))
            or before[TRAIT_PATH_FILE]['sha256'] != TRAIT_PATH_OFFICIAL_SHA
            or after[comparison.TYPE_FILE]['sha256'] != TRAIT_TYPE_SHA
            or after[TRAIT_PATH_FILE]['sha256'] != TRAIT_PATH_SHA
            or any(before[p]['mode'] != after[p]['mode'] for p in changed)):
        raise RuntimeError('candidate differs from the fixed trait visibility annotation patch')
    return dict(variant=V5_VARIANT, base_variant=VARIANT, patch_sha256=TRAIT_PATCH_SHA,
                base_receipts_sha256={name: runtime.digest(out / name) for name in
                    ('arity-patch-identity.json', 'lines-model-identity.json')},
                changed_files=changed, before_files=before, candidate_files=after)


def prepare_trait_visibility(closure, work, out, end, run):
    """Apply the fixed annotations after verifying the genuine v3 intermediate."""
    original, private = Path(closure['codeql/rust-all']['root']), work / 'candidate-rust-all'
    verify_candidate_identity(original, private, out, end)
    patch_file = HERE / 'codeql-rust-trait-visibility.patch'
    if (runtime.digest(patch_file) != TRAIT_PATCH_SHA
            or runtime.digest(private / TRAIT_PATH_FILE) != TRAIT_PATH_OFFICIAL_SHA):
        raise RuntimeError('trait visibility patch or base path identity differs')
    run(['patch', '--batch', '--fuzz=0', '-p1', '-d', private, '-i', patch_file],
        'apply-trait-visibility-patch', 20)
    runtime.save(out / 'trait-visibility-identity.json',
                 trait_visibility_identity(original, private, out, end), out)


def lines_model_bytes(path, end):
    capacity.check_time(end)
    if path.resolve() != path or runtime.digest(path) != MODEL_OFFICIAL_SHA:
        raise RuntimeError('official core model identity differs')
    original_bytes = path.read_bytes()
    if original_bytes.count(MODEL_MARKER) != 1:
        raise RuntimeError('core model insertion point is not unique')
    candidate_bytes = original_bytes.replace(MODEL_MARKER, MODEL_MARKER + MODEL_LINE, 1)
    if hashlib.sha256(candidate_bytes).hexdigest() != MODEL_CANDIDATE_SHA:
        raise RuntimeError('lines summary target identity differs')
    return candidate_bytes


def prepare_lines_model(original, private, out, end):
    path = private / MODEL_FILE
    path.write_bytes(lines_model_bytes(path, end))
    runtime.save(out / 'lines-model-identity.json', lines_identity(original, private, end), out)


def suite_helper(official):
    # This exact dependency is a query pack. Keep the older library-only parser strict.
    root = official / 'qlpacks/codeql/suite-helpers/1.0.61'
    manifest = root / 'qlpack.yml'
    text = manifest.read_text()
    for key, wanted in (('name', 'codeql/suite-helpers'), ('version', '1.0.61'),
                        ('library', 'false'), ('dependencies', '{}')):
        if re.findall(r'^' + key + r': (.*)$', text, re.M) != [wanted]:
            raise RuntimeError('suite-helpers role or identity differs')
    if manifest.resolve() != manifest:
        raise RuntimeError('unsafe suite-helpers manifest')
    return dict(name='codeql/suite-helpers', version='1.0.61', library=False,
                dependencies={}, manifest_sha256=runtime.digest(manifest), root=str(root))


def freeze_suite(suite):
    entries = suite.get('queries')
    digest = hashlib.sha256(json.dumps(entries, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    if not isinstance(entries, list) or len(entries) != 37 or digest != DEFAULT_ENTRIES_SHA or suite.get('suite_sha256') != SUITE_SHA:
        raise RuntimeError('default workload differs from the fixed original 37 queries')
    return entries


def query_records(work, source, suite, frozen_witnesses):
    default, witness = work / 'default-queries', work / 'public-witnesses'
    texts = capacity.semantic_queries(source, image_version=IMAGE_CONTRACT)
    if len(texts) != 7 or {n: hashlib.sha256(t.encode()).hexdigest() for n, t in texts.items()} != frozen_witnesses:
        raise RuntimeError('public witness text differs from the image-v2 seven-table contract')
    records = [dict(entry, kind='default', pack=DEFAULT_PACK, path=str(default / entry['relative']), root=str(default))
               for entry in freeze_suite(suite)]
    records.extend(dict(relative=n + '.ql', id=n, sha256=frozen_witnesses[n], kind='witness',
                        pack=WITNESS_PACK, path=str(witness / (n + '.ql')), root=str(witness)) for n in texts)
    for record in records:
        path, root = Path(record['path']), Path(record['root'])
        if path.resolve() != path or not path.is_relative_to(root) or runtime.digest(path) != record['sha256']:
            raise RuntimeError('query input differs from the frozen 37+7 source')
    return records


def source_layout(official, work, out, source, suite, frozen_witnesses, end):
    """Copy original QL/support bytes, replacing only published packaging/bindings."""
    freeze_suite(suite)
    published = official / 'qlpacks/codeql/rust-queries/0.1.46'
    if suite.get('query_root') != str(published):
        raise RuntimeError('official query pack path differs')
    default, witness = work / 'default-queries', work / 'public-witnesses'
    default.mkdir(); witness.mkdir()
    packaging = dict(copied_source_files={}, excluded_published_files={}, replacement_metadata={})
    for relative, identity in regular_snapshot(published, end).items():
        capacity.check_time(end)
        path = Path(relative)
        if path.parts[0] == '.codeql': reason = 'published embedded libraries or compilation material'
        elif path.suffix == '.qlx': reason = 'published precompiled query'
        elif relative in ('.packinfo', 'codeql-pack.lock.yml'): reason = 'published packaging/dependency binding'
        elif relative == 'qlpack.yml': reason = 'explicit private source-pack wrapper'
        else: reason = None
        if reason:
            packaging['excluded_published_files'][relative] = dict(identity, reason=reason)
        else:
            dest = default / relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(published / relative, dest)
            if capacity.digest_until(dest, end) != identity['sha256']:
                raise RuntimeError('source copy differs')
            packaging['copied_source_files'][relative] = identity
    manifest = ('name: ' + DEFAULT_PACK + '\nversion: 0.0.0\nlibrary: false\nwarnOnImplicitThis: true\n'
                'dependencies:\n  codeql/rust-all: 0.2.25\n  codeql/suite-helpers: 1.0.61\n  codeql/util: 2.0.48\n'
                'extractor: rust\ndefaultSuiteFile: codeql-suites/rust-code-scanning.qls\n')
    (default / 'qlpack.yml').write_text(manifest)
    packaging['replacement_metadata']['qlpack.yml'] = dict(text=manifest, sha256=runtime.digest(default / 'qlpack.yml'))
    runtime.save(out / 'packaging-differences.json', packaging, out)
    if capacity.digest_until(default / 'codeql-suites/rust-code-scanning.qls', end) != SUITE_SHA:
        raise RuntimeError('source suite changed')
    texts = capacity.semantic_queries(source, image_version=IMAGE_CONTRACT)
    if len(texts) != 7 or {name: hashlib.sha256(text.encode()).hexdigest() for name, text in texts.items()} != frozen_witnesses:
        raise RuntimeError('public witness text differs from the image-v2 seven-table contract')
    (witness / 'qlpack.yml').write_text('name: ' + WITNESS_PACK + '\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.25\n')
    for name, text in texts.items():
        path = witness / (name + '.ql'); path.write_text(text)
    records = query_records(work, source, suite, frozen_witnesses)
    runtime.save(out / 'query-inputs.json', records, out)
    return records


def query_options(closure, work, prefix):
    return ['--additional-packs=' + os.pathsep.join(p['root'] for p in closure.values()),
            '--no-default-compilation-cache', '--compilation-cache=' + str(work / (prefix + '-compilation-cache')),
            '--common-caches=' + str(work / (prefix + '-common-cache'))]


def resolve_layout(codeql, records, closures, work, out, run):
    options = query_options(closures['default'], work, 'resolve')
    run([codeql, 'resolve', 'queries', work / 'default-queries/codeql-suites/rust-code-scanning.qls',
         '--format=json', options[0], options[-1]], 'private-default-suite', 30)
    actual = capacity.suite_entries(read_json(out / 'private-default-suite.log'), work / 'default-queries')
    expected = [{key: r[key] for key in ('relative', 'id', 'sha256')} for r in records if r['kind'] == 'default']
    if actual != expected:
        raise RuntimeError('private source wrapper changed the default suite selection')
    receipts = []
    for index, record in enumerate(records):
        name = 'resolve-' + str(index).zfill(2)
        run([codeql, 'resolve', 'library-path', '--query=' + record['path'], '--format=json', *options], name, 20)
        actual = read_json(out / (name + '.log'))
        capacity.bundle.verify_library_path(actual, Path(record['root']), str(Path(record['relative']).with_suffix('')),
                                           closures[record['kind']], record['pack'])
        if actual['compilationCache'] != [str(work / 'resolve-compilation-cache')]:
            raise RuntimeError('resolver selected an unbound compilation cache')
        receipts.append(dict(query=record['path'], kind=record['kind'], actual=actual, status='PASS_CANDIDATE_CLOSURE'))
        runtime.save(out / 'library-resolution.json', receipts, out)


def admit_checkpoint(captured, finished, source, deadline):
    for phase, value in (('capture', captured), ('finish', finished)):
        if (value.get('phase') != phase or value.get('mode') != MODE or value.get('source') != capacity.CAPACITY_SOURCE
                or value.get('source_root') != str(source) or value.get('fixed_job_deadline') != deadline
                or value.get('expected_bundle_version') != '2.27.4' or value.get('analyze_outcome') != 'success'):
            raise RuntimeError('official checkpoint identity differs')
    if (captured.get('status') != 'CAPTURED' or captured.get('evidence_status') != 'COMPLETE'
            or captured.get('cargo_guard', {}).get('status') != 'PASS'
            or captured.get('coverage_capture', {}).get('status') != 'PASS'
            or any(captured.get(k) != 'PASS' for k in ('checkout_gate', 'lock_observation_gate', 'resource_gate'))
            or finished.get('status') != 'CANDIDATE_EXTRACTION_READY'
            or finished.get('assessment_status') != 'CANDIDATE_RESEARCH_READY'
            or finished.get('absolute_action_deadline', {}).get('status') != 'PASS'
            or finished.get('verification_admitted') is not True
            or any(finished.get(k) != 'PASS' for k in ('extraction_checkpoint', 'archived_source'))
            or any(finished.get(k) != 'NOT_REQUESTED' for k in ('default_result_verification', 'public_semantics'))
            or any(finished.get('after_identity', {}).get(k) != 'UNCHANGED' for k in ('source', 'official'))):
        raise RuntimeError('official extraction checkpoint was not admitted')


def result_paths(records, database):
    return {str(i).zfill(2): database / 'results' / r['pack'] / Path(r['relative']).with_suffix('.bqrs')
            for i, r in enumerate(records)}


def preserve_results(results, out, end):
    saved = {}
    # Save the disposition after each file, retaining partial capture on failure.
    for label, path in results.items():
        capacity.check_time(end)
        if path.resolve() != path:
            raise RuntimeError('unsafe result path')
        saved[label] = dict(path=str(path), status='NOT_PRODUCED')
        runtime.save(out / 'raw-results.json', saved, out)
        if not path.exists(): continue
        size = path.stat().st_size
        if not path.is_file() or size > runtime.WRITE_CAP - runtime.output_bytes(out):
            raise RuntimeError('raw BQRS exceeds remaining private evidence bound')
        data = path.read_bytes()
        runtime.evidence_write(out / ('result-' + label + '.bqrs'), data, out)
        saved[label].update(status='SAVED_UNVERIFIED', bytes=size, sha256=hashlib.sha256(data).hexdigest())
        runtime.save(out / 'raw-results.json', saved, out)


def verify_results(codeql, database, records, results, source, provenance, work, out, resources, closure, run, terminal):
    terminal['default_result_verification'] = 'RUNNING'
    manifest = []
    for index, record in enumerate(records[:37]):
        label = str(index).zfill(2); result = results[label]
        if not result.is_file(): raise RuntimeError('missing default BQRS: ' + record['id'])
        run([codeql, 'bqrs', 'info', result, '--format=json'], 'info-' + label, 20)
        run([codeql, 'bqrs', 'decode', result, '--format=json'], 'decoded-' + label, 30)
        info, decoded = read_json(out / ('info-' + label + '.log')), read_json(out / ('decoded-' + label + '.log'), PRIVATE_CAP)
        capacity.verify_decoded(info, decoded)
        manifest.append(dict(record, result_info=info, bqrs_sha256=runtime.digest(result), bqrs_bytes=result.stat().st_size))
        runtime.save(out / 'default-query-results.json', manifest, out)
    sarif = work / 'default-results.sarif'
    run([codeql, 'database', 'interpret-results', '--format=sarifv2.1.0', '--output=' + str(sarif),
         '--threads=' + str(resources['CODEQL_THREADS']), query_options(closure, work, 'batch')[0],
         '--common-caches=' + str(work / 'batch-common-cache'), '--', database,
         *[r['path'] for r in records[:37]]], 'interpret-default-results')
    # The whole SARIF remains runner-local. Only bounded verification metadata is exportable.
    findings = capacity.check_sarif(read_json(sarif, 256 * 1024**2))
    runtime.save(out / 'local-sarif-verification.json', dict(status='PASS', findings=findings,
                 bytes=sarif.stat().st_size, sha256=runtime.digest(sarif), queries=37,
                 artifact_contains_sarif=False, security_upload=False), out)
    terminal.update(default_result_verification='PASS', default_queries=37, findings=findings, public_semantics='RUNNING')
    validated, failures = {}, {}
    terminal['public_semantic_tables'], terminal['public_semantic_failures'] = validated, failures
    for index, record in enumerate(records[37:], 37):
        name, label = record['id'], str(index).zfill(2)
        run([codeql, 'bqrs', 'decode', results[label], '--format=csv'], 'decode-' + name, 30)
        rows = witness_rows(name, out / ('decode-' + name + '.log'))
        try:
            validated[name] = capacity.validate(name, rows, source, provenance, image_version=IMAGE_CONTRACT)
        except RuntimeError as error:
            failures[name] = str(error)
            if terminal['primary_failure'] is None:
                terminal['primary_failure'] = dict(stage='semantic-' + name, error=str(error))
            runtime.save(out / ('semantic-' + name + '-terminal.json'), dict(status='FAIL', error=str(error),
                         image_contract_version=IMAGE_CONTRACT if name == 'Image' else None), out)
        else:
            runtime.save(out / ('semantic-' + name + '-terminal.json'), dict(status='PASS', validated=validated[name],
                         image_contract_version=IMAGE_CONTRACT if name == 'Image' else None), out)
    terminal['public_semantics'] = 'FAIL' if failures else 'PASS'
    if failures:
        raise RuntimeError('public semantic validation failed: ' + ', '.join(failures))


def seal(out, terminal, phase, deadline, end):
    paths = list(out.rglob('*'))
    if any(p.is_symlink() or not (p.is_file() or p.is_dir()) or p.suffix == '.sarif' for p in paths):
        raise RuntimeError('unsafe or forbidden private evidence')
    # Hash all nonterminal evidence first; charge that work before judging margin.
    terminal_name = 'candidate-' + phase + '-terminal.json'
    hashes = {str(p.relative_to(out)): capacity.digest_until(p, end) for p in sorted(paths)
              if p.is_file() and p.name not in ('hashes.json', terminal_name)}
    terminal['remaining_job_seconds'] = deadline - time.time()
    if phase == 'run' and terminal['status'] == 'CANDIDATE_RESEARCH_COMPLETED' and terminal['remaining_job_seconds'] < REQUIRED_MARGIN:
        terminal['status'] = 'CANDIDATE_RESEARCH_NEAR_LIMIT'
    runtime.save(out / terminal_name, terminal, out, terminal=True)
    hashes[terminal_name] = runtime.digest(out / terminal_name)
    runtime.save(out / 'hashes.json', hashes, out, terminal=True)
    if phase == 'run' and terminal['status'] == 'CANDIDATE_RESEARCH_COMPLETED' and deadline - time.time() < REQUIRED_MARGIN:
        terminal.update(status='CANDIDATE_RESEARCH_NEAR_LIMIT', remaining_job_seconds=deadline - time.time())
        runtime.save(out / terminal_name, terminal, out, terminal=True)
        hashes[terminal_name] = runtime.digest(out / terminal_name)
        runtime.save(out / 'hashes.json', hashes, out, terminal=True)


def _main(cancellation, owned_resources):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('phase', choices=('prepare', 'run'))
    for name in ('codeql', 'source', 'database', 'official-evidence', 'output', 'work'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--deadline', type=float, required=True)
    parser.add_argument('--analysis-deadline', type=float, required=True)
    for stage in ('candidate-prepare', 'extraction', 'capture', 'checkpoint', 'official-artifact'):
        parser.add_argument('--' + stage + '-outcome', default='unknown')
    args = parser.parse_args()
    source, official, database = args.source.resolve(), args.codeql.parent.resolve(), args.database.resolve()
    out, work, checkpoint = args.output.absolute(), args.work.absolute(), args.official_evidence.resolve()
    runner = Path(os.environ['RUNNER_TEMP']).resolve()
    for path in (out, work):
        if (path.resolve() != path or not path.is_relative_to(runner) or path == runner
                or any(path.is_relative_to(p) or p.is_relative_to(path) for p in (source, official, database, checkpoint))):
            raise RuntimeError('candidate paths overlap protected inputs')
    if out.is_relative_to(work) or work.is_relative_to(out):
        raise RuntimeError('candidate evidence and work overlap')
    out.mkdir(exist_ok=args.phase == 'run')
    runtime.CAP, runtime.WRITE_CAP = PRIVATE_CAP, PRIVATE_CAP - 1024**2
    started = time.monotonic()
    end = started + max(0, args.deadline - time.time()) if math.isfinite(args.deadline) else started
    terminal = dict(status='CANDIDATE_RESEARCH_INCOMPLETE', phase=args.phase, mode=MODE,
                    candidate_variant=WORKLOAD_VARIANT, image_contract_version=IMAGE_CONTRACT,
                    source=capacity.CAPACITY_SOURCE, source_root=str(source), fixed_job_deadline=args.deadline,
                    analysis_deadline=args.analysis_deadline, production_capacity='UNKNOWN', full_rust_coverage='HOLD',
                    production_acceptance='UNCHANGED', security_upload=False, database_upload=False, sarif_upload=False,
                    default_result_verification='NOT_REACHED', public_semantics='NOT_REACHED',
                    performance_observation={'status': 'NOT_REQUESTED' if args.phase == 'prepare' else 'NOT_REACHED'},
                    primary_failure=None, secondary_errors=[], stage='admission',
                    final_hosted_job_margin='NOT_OBSERVED', required_final_job_margin_seconds=REQUIRED_MARGIN,
                    margin_scope='pre-upload evidence readiness; final hosted completion including upload/post requires independent readback')
    before = protected = official_seal = db_sha = None
    observation = batch_error = None
    env = {k: v for k, v in os.environ.items() if k not in ('GH_TOKEN', 'GITHUB_TOKEN', 'RUSTC_WRAPPER')}
    def save(name, value, control=False): runtime.save(out / name, value, out, terminal=control)
    def run(argv, name, limit=None, *, analysis=False, observed=False):
        terminal['stage'] = name
        if cancellation['cancelled']:
            raise runtime.CancelledError('CANCELLED_BEFORE_SPAWN')
        child_end = min(end - EVIDENCE_SECONDS, analysis_end) if analysis else end - EVIDENCE_SECONDS
        remaining = child_end - time.monotonic()
        if remaining <= 0: raise TimeoutError('fixed candidate deadline exhausted before ' + name)
        if observed:
            return observation.run_batch(list(map(str, argv)), source, child_end,
                                         min(remaining, limit or remaining), env)
        return runtime.run(list(map(str, argv)), source, out / (name + '.log'), child_end,
                           min(remaining, limit or remaining), out, env, file_limit=None)
    try:
        budget = read_json(checkpoint / 'capacity-budget.json')
        analysis_end, end = shared_budget(budget, args.deadline, args.analysis_deadline, time.time(), time.monotonic())
        terminal['budget'] = budget
        contract = capacity.semantic_contract(source, image_version=IMAGE_CONTRACT)
        if (read_json(checkpoint / 'semantic-query-contract.json') != contract
                or read_json(checkpoint / 'semantic-query-inputs.json') != contract['query_sha256']):
            raise RuntimeError('candidate public query contract differs from the frozen preparation')
        terminal['semantic_contract_sha256'] = runtime.digest(checkpoint / 'semantic-query-contract.json')
        if args.phase == 'run':
            outcomes = {stage: getattr(args, stage.replace('-', '_') + '_outcome') for stage in
                        ('candidate-prepare', 'extraction', 'capture', 'checkpoint', 'official-artifact')}
            terminal['workflow_outcomes'] = outcomes
            if any(value != 'success' for value in outcomes.values()):
                raise RuntimeError('prior stage incomplete; no workload subprocess admitted')
            # Verify the earlier private receipt before appending result evidence.
            comparison.verify_seal(out, end - EVIDENCE_SECONDS)
            prepared = read_json(out / 'candidate-prepare-terminal.json')
            if (prepared.get('status') != 'CANDIDATE_RESEARCH_PREPARED' or prepared.get('budget') != budget
                    or prepared.get('source_root') != str(source) or prepared.get('candidate_variant') != WORKLOAD_VARIANT
                    or prepared.get('image_contract_version') != IMAGE_CONTRACT
                    or prepared.get('semantic_contract_sha256') != terminal['semantic_contract_sha256']):
                raise RuntimeError('candidate preparation receipt differs')
            before = read_json(out / 'private-inputs-before.json')
        else:
            work.mkdir(exist_ok=False)
            prepared = read_json(checkpoint / 'capacity-prepare-terminal.json')
            if (prepared.get('status') != 'PREPARED' or prepared.get('mode') != MODE
                    or prepared.get('source') != capacity.CAPACITY_SOURCE or prepared.get('source_root') != str(source)
                    or prepared.get('fixed_job_deadline') != args.deadline
                    or prepared.get('image_contract_version') != IMAGE_CONTRACT):
                raise RuntimeError('official preparation was not admitted')
        capacity.check_time(analysis_end)
        terminal['stage'] = 'protected-input-admission'
        protected = capacity.after_identity(source, official, checkpoint, end=min(analysis_end, end - EVIDENCE_SECONDS))
        save('protected-inputs-' + args.phase + '-before.json', protected)
        if any(protected.get(k) != 'UNCHANGED' for k in ('source', 'official')):
            raise RuntimeError('business source or official bundle changed')
        resources = capacity.resources(env)
        recorded = read_json(checkpoint / 'resources.json')
        if any(resources[k] != recorded[k] for k in ('CODEQL_RAM', 'CODEQL_THREADS')):
            raise RuntimeError('Action resource selection differs')
        save('resources-' + args.phase + '.json', resources)
        original_closure = capacity.bundle.bundle_closure(official, '2.27.4')
        if original_closure != read_json(checkpoint / 'library-closure.json'):
            raise RuntimeError('official library closure differs')
        if args.phase == 'prepare':
            terminal['stage'] = 'prepare-candidate-library'
            prepare_candidate(original_closure, work, out, analysis_end,
                lambda argv, name, limit: run(argv, name, limit, analysis=True), copy_end=analysis_end)
            terminal['stage'] = 'prepare-shared-helper'
            prepare_shared_helper(original_closure, work, out, analysis_end,
                lambda argv, name, limit: run(argv, name, limit, analysis=True), copy_end=analysis_end)
            terminal['stage'] = 'prepare-trait-visibility'
            prepare_trait_visibility(original_closure, work, out, analysis_end,
                lambda argv, name, limit: run(argv, name, limit, analysis=True))
            terminal['stage'] = 'prepare-constraint-matching'
            prepare_constraint_matching(original_closure, work, out, analysis_end,
                lambda argv, name, limit: run(argv, name, limit, analysis=True))
            closure = workload_closure(original_closure, work)
            defaults = dict(closure, **{'codeql/suite-helpers': suite_helper(official)})
            if len(closure) != 13 or len(defaults) != 14: raise RuntimeError('unexpected dependency closure')
            closures = dict(default=defaults, witness=closure)
            save('selected-closures.json', closures)
            terminal['stage'] = 'source-layout'
            records = source_layout(official, work, out, source, read_json(checkpoint / 'default-suite.json'),
                                    read_json(checkpoint / 'semantic-query-inputs.json'), analysis_end)
            before = private_inputs(work, analysis_end)
            save('private-inputs-before.json', before)
            resolve_layout(args.codeql, records, closures, work, out,
                           lambda argv, name, limit: run(argv, name, limit, analysis=True))
            terminal.update(status='CANDIDATE_RESEARCH_PREPARED', resolved_default_queries=37, resolved_public_witnesses=7)
        else:
            terminal['stage'] = 'candidate-input-admission'
            closures = read_json(out / 'selected-closures.json')
            records = read_json(out / 'query-inputs.json')
            expected = query_records(work, source, read_json(checkpoint / 'default-suite.json'),
                                     read_json(checkpoint / 'semantic-query-inputs.json'))
            witness_closure = workload_closure(original_closure, work)
            if (records != expected or closures != dict(witness=witness_closure,
                    default=dict(witness_closure, **{'codeql/suite-helpers': suite_helper(official)}))
                    or runtime.digest(work / 'candidate-rust-all' / comparison.TYPE_FILE) != TRAIT_TYPE_SHA):
                raise RuntimeError('candidate workload binding differs from the frozen inputs')
            verify_workload_identity(original_closure, work, out, min(analysis_end, end - EVIDENCE_SECONDS))
            if private_inputs(work, min(analysis_end, end - EVIDENCE_SECONDS)) != before:
                raise RuntimeError('private input changed after preparation')
            official_seal = comparison.verify_seal(checkpoint, min(analysis_end, end - EVIDENCE_SECONDS))
            save('official-evidence-before.json', official_seal)
            captured, finished = (read_json(checkpoint / ('capacity-' + phase + '-terminal.json')) for phase in ('capture', 'finish'))
            terminal['stage'] = 'extraction-checkpoint-admission'
            admit_checkpoint(captured, finished, source, args.deadline)
            run([args.codeql, 'resolve', 'database', database, '--format=json'], 'database-identity', 20, analysis=True)
            db_sha = capacity.finalized_database_identity(read_json(out / 'database-identity.log'), source, database)
            if db_sha != finished.get('database_metadata_sha256'): raise RuntimeError('database differs from extraction checkpoint')
            terminal['database_metadata_sha256'] = db_sha
            results = result_paths(records, database)
            if (database / 'results').is_symlink() or any(p.is_symlink() or p.suffix == '.bqrs' for p in (database / 'results').rglob('*')):
                raise RuntimeError('extraction checkpoint already has query results')
            for name in ('batch-compilation-cache', 'batch-common-cache'): (work / name).mkdir(exist_ok=False)
            terminal['stage'] = 'performance-observation-admission'
            observation = candidate_profile.Observation(out, work, records)
            owned_resources.callback(observation.close)
            observation.start()
            terminal['batch'] = dict(status='RUNNING', queries=44, default_queries=37, public_witnesses=7, invocations=1)
            save('candidate-run-terminal.json', terminal, True)
            batch_error = None
            try:
                run([args.codeql, 'database', 'run-queries', *query_options(closures['default'], work, 'batch'),
                     '--no-use-precompiled', '--threads=' + str(resources['CODEQL_THREADS']), '--ram=' + str(resources['CODEQL_RAM']),
                     '--', database, *[r['path'] for r in records]], 'candidate-workload-batch', analysis=True, observed=True)
                terminal['batch']['status'] = 'COMPUTED'
            except Exception as error:
                batch_error = error
                terminal['primary_failure'] = dict(stage=terminal['stage'], error=repr(error)[:4096])
                terminal['batch']['status'] = 'INCOMPLETE'
                save('candidate-run-terminal.json', terminal, True)
                raise
            finally:
                terminal['raw_result_preservation'] = 'INCOMPLETE'
                try:
                    preserve_results(results, out, end - EVIDENCE_SECONDS)
                    terminal['raw_result_preservation'] = 'COMPLETE'
                except Exception as error:
                    if batch_error is None: raise
                    terminal['secondary_errors'].append(dict(stage='raw-results', error=repr(error)[:2048]))
            verify_results(args.codeql, database, records, results, source, read_json(checkpoint / 'libp2p-provenance.json'),
                           work, out, resources, closures['default'], run, terminal)
            terminal.update(status='CANDIDATE_RESEARCH_COMPLETED')
    except Exception as error:
        terminal['status'] = 'CANDIDATE_RESEARCH_INCOMPLETE'
        if terminal['primary_failure'] is None: terminal['primary_failure'] = dict(stage=terminal['stage'], error=repr(error)[:4096])
        for key in ('default_result_verification', 'public_semantics'):
            if terminal[key] == 'RUNNING': terminal[key] = 'INCOMPLETE'
    finally:
        # Core closeout precedes the single optional summary. Interrupts cannot skip it.
        save('candidate-' + args.phase + '-terminal.json', terminal, True)
        for name, operation in (
            ('private_inputs', lambda: private_inputs(work, end - EVIDENCE_SECONDS) if before is not None else None),
            ('protected_inputs', lambda: capacity.after_identity(source, official, checkpoint, end=end - EVIDENCE_SECONDS) if protected is not None else None),
            ('official_evidence', lambda: comparison.verify_seal(checkpoint, end - EVIDENCE_SECONDS) if official_seal is not None else None),
            ('database_metadata', lambda: capacity.digest_until(database / 'codeql-database.yml', end - EVIDENCE_SECONDS) if db_sha else None),
        ):
            try:
                actual = operation()
                expected = {'private_inputs': before, 'protected_inputs': protected, 'official_evidence': official_seal, 'database_metadata': db_sha}[name]
                if actual is not None: save(name + '-' + args.phase + '-after.json', actual, True)
                if actual != expected: raise RuntimeError(name + ' changed during candidate research')
                terminal[name + '_after'] = 'UNCHANGED' if expected is not None else 'NOT_REACHED'
            except Exception as error:
                terminal['status'] = 'CANDIDATE_RESEARCH_INCOMPLETE'
                failure = dict(stage=name + '-after', error=repr(error)[:2048])
                if terminal['primary_failure'] is None: terminal['primary_failure'] = failure
                else: terminal['secondary_errors'].append(failure)
        if observation is not None:
            try:
                original_failure_allows_summary = (terminal['primary_failure'] is None or
                    isinstance(batch_error, runtime.PhaseError) and batch_error.clean_timeout
                    and terminal['primary_failure']['stage'] == 'candidate-workload-batch')
                unchanged = all(terminal.get(name + '_after') == 'UNCHANGED' for name in
                                ('private_inputs', 'protected_inputs', 'official_evidence', 'database_metadata'))
                observation.summarize(args.codeql, source, env, end - EVIDENCE_SECONDS,
                    admitted=original_failure_allows_summary and unchanged and not terminal['secondary_errors']
                             and terminal.get('raw_result_preservation') == 'COMPLETE' and not cancellation['cancelled'])
                terminal['performance_observation'] = observation.finish()
                if terminal['performance_observation']['status'] != 'COMPLETE':
                    raise RuntimeError('PERFORMANCE_OBSERVATION_INCOMPLETE')
            except Exception:
                terminal['status'] = 'CANDIDATE_RESEARCH_INCOMPLETE'
                failure = dict(stage='performance-observation', error='PERFORMANCE_OBSERVATION_INCOMPLETE')
                if terminal['primary_failure'] is None: terminal['primary_failure'] = failure
                else: terminal['secondary_errors'].append(failure)
            finally:
                try:
                    observation.close()
                except Exception:
                    terminal['status'] = 'CANDIDATE_RESEARCH_INCOMPLETE'
                    failure = dict(stage='performance-observation-cleanup', error='CAPTURE_CLOSE_FAILED')
                    if terminal['primary_failure'] is None: terminal['primary_failure'] = failure
                    else: terminal['secondary_errors'].append(failure)
        if cancellation['cancelled']:
            terminal['cancelled'] = True
            terminal['status'] = 'CANDIDATE_RESEARCH_INCOMPLETE'
            if terminal['primary_failure'] is None:
                terminal['primary_failure'] = dict(stage=terminal['stage'], error='CANCELLED')
        terminal['elapsed_seconds'] = time.monotonic() - started
        sealed = False
        try:
            seal(out, terminal, args.phase, args.deadline, end - EVIDENCE_SECONDS)
            sealed = True
        except Exception as error:
            terminal['status'] = 'CANDIDATE_RESEARCH_INCOMPLETE'
            failure = dict(stage='evidence-seal', error=repr(error)[:2048])
            if terminal['primary_failure'] is None: terminal['primary_failure'] = failure
            else: terminal['secondary_errors'].append(failure)
            save('candidate-' + args.phase + '-terminal.json', terminal, True)
        if cancellation['cancelled'] and not terminal.get('cancelled'):
            terminal.update(status='CANDIDATE_RESEARCH_INCOMPLETE', cancelled=True)
            if terminal['primary_failure'] is None:
                terminal['primary_failure'] = dict(stage=terminal['stage'], error='CANCELLED')
            filename = 'candidate-' + args.phase + '-terminal.json'
            save(filename, terminal, True)
            if sealed:
                hashes = read_json(out / 'hashes.json')
                hashes[filename] = runtime.digest(out / filename)
                save('hashes.json', hashes, True)
    return 0 if not cancellation['cancelled'] and terminal['status'] in ('CANDIDATE_RESEARCH_PREPARED', 'CANDIDATE_RESEARCH_COMPLETED') else 1


def main():
    with runtime.cancellation_guard() as cancellation, ExitStack() as owned_resources:
        return _main(cancellation, owned_resources)


if __name__ == '__main__':
    raise SystemExit(main())
