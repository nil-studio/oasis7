#!/usr/bin/env python3
"""Advisory core headings and explicit feature cards, not semantic truth verification.

Scope uses the existing product/system selectors. A selected product file is
scanned in full, including legacy REQ blocks; absence is a finding, not coverage.
No remote links are fetched and no implementation status is inferred from Git.
"""
from __future__ import annotations
import argparse
import importlib.util
from functools import lru_cache
from pathlib import Path
import re
import sys
from doc_content_warnings import report_findings, report_classified_findings, finding_code
from markdown_it import MarkdownIt


def load_checker(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


product = load_checker('product-doc-content-check')
system = load_checker('system-design-traceability-check')
STATUS_FIELDS = ('系统设计状态', '实现状态', '系统设计依据', '实现与验证依据', '核对日期与基线', '剩余范围')
DESIGN_STATUS = {'未核实', '未设计', '部分覆盖', '已覆盖', '不适用'}
IMPLEMENTATION_STATUS = {'未核实', '未实现', '部分实现', '已实现待验证', '已验证'}
ADVISORY_CODES = frozenset({'missing-core-section', 'invalid-structure-tailoring', 'missing-status-field'})


def finding(path, line, code, message):
    return f'doc-core-content: {code}: {path}:{line}: {message}'


def check_core(path, text, previous_text=None):
    lines = product.visible_lines(text)
    headings = '\n'.join(line for _, line in lines if re.match(r'^ {0,3}#{1,6}\s', line))
    # Root product PRDs retain their separate, established root contract.
    if path.endswith('/prd.md'):
        return []
    if path.startswith('doc/product/') and path.endswith('.prd.md'):
        groups = ('文档身份', '问题|目标', '范围|Non-Goals', '体验|决策', '要求', '验收|证据', '未决|取舍|风险')
    elif path.startswith('doc/product/'):
        groups = ('文档身份', '设计命题|问题|目标', '代表性|流程|体验', '机制|设计', '验证|追踪|证据', '决策|取舍|未决|风险')
    elif system.is_module_design_overview(path) and not system.design_requires_traceability(path, text, previous_text):
        # Module overview duties come from doc-structure-standard section 3.2;
        # do not impose topic trace tables or feature cards on navigation.
        groups = ('架构|结构|设计定位|Architecture|Structure', '组件|职责|分层|Component|Responsibilit', '主链路|流程|Flow|Pipeline', '数据流|状态流|Data Flow|State Flow', '导航|阅读顺序|专题|Navigation|Reading')
    else:
        groups = ('问题|目标|范围', '上游|需求承接', '边界|结构|设计条款', '验证', '决策|未决|风险')
    tailoring = [(number, line) for number, line in lines if re.match(r'^\s*-?\s*结构裁剪[:：]', line)]
    if tailoring:
        number, line = tailoring[0]
        values = {key: re.search(key + r'[:：]\s*([^；;]+)', line) for key in ('理由', '范围', '风险', '映射')}
        complete = all(match and match[1].strip() not in {'无', '-', 'N/A', '待补'} for match in values.values())
        mapping = values['映射'][1] if values['映射'] else ''
        mappings = []
        for token in MarkdownIt('commonmark').parse(mapping):
            target, label = None, ''
            for child in token.children or []:
                if child.type == 'link_open':
                    target, label = child.attrGet('href'), ''
                elif child.type == 'link_close':
                    link_path, fragment = product.split_link_target(target or '')
                    if not link_path and fragment and product.fragment_exists(text, fragment):
                        mappings.append(label)
                    target = None
                elif target is not None:
                    label += child.content
        has_valid_mapping = bool(mappings)
        missing = [group for group in groups if not re.search(group, headings, re.I)]
        unmapped = []
        for group in missing:
            match = next((i for i, label in enumerate(mappings) if re.search(group, label, re.I)), None)
            if match is None:
                unmapped.append(group)
            else:
                mappings.pop(match)  # Each missing group needs its own labelled link.
        if complete and not unmapped and has_valid_mapping:
            return []
        return [finding(path, number, 'invalid-structure-tailoring', '裁剪须有具体理由、范围、风险；每个缺失核心组分别用匹配该组的链接标签映射到本文件真实章节。缺映射：' + ', '.join(unmapped))]
    return [finding(path, 1, 'missing-core-section', f'缺少核心章节（可使用等价标题或说明结构裁剪）：{group}') for group in groups if not re.search(group, headings, re.I)]


def _design_relations_reader(root, read_target):
    """Reuse validated rows only within this reader's content snapshot.

    Relation records and the returned tuple are immutable. Baseline/current
    readers must have separate instances, as must separate checker invocations.
    Every row still passes the original relation validation before it can match
    a feature; invalid rows are never promoted to usable evidence.
    """
    @lru_cache(maxsize=128)
    def read(target):
        table, _, _ = system.traceability_tables(read_target(target))
        relations = []
        for row_number, cells in table.rows:
            row_errors = []
            relation = system.relation_from_demand_row(
                root, target, row_number, cells, row_errors, False, read_target)
            if relation is not None and not any(finding_code(error) not in system.ADVISORY_CODES for error in row_errors):
                relations.append(relation)
        return tuple(relations)
    return read


def check_design_evidence(path, number, block, evidence, root, read_target, *, read_design_relations=None):
    """Check exact mechanical relations; semantic completeness remains human review."""
    if read_design_relations is None:
        read_design_relations = _design_relations_reader(root, read_target)
    errors = []
    source = root / path
    identities = system.declared_trace_ids(block)
    trace_refs, _invalid_refs = system.trace_reference_ids(block)
    identities.update(trace_refs)
    for _, line in product.visible_lines(block):
        match = re.match(r'^ {0,3}#{1,6}\s+((?:REQ|SC|AC)-[A-Z0-9_-]+)\b', line, re.I)
        if match:
            identities.add(match[1].upper())
    # A feature may be implemented through its explicitly associated acceptance criteria.
    source_text = read_target(source) or block
    _, _, acceptance_relations = product.declared_prd_relations(source_text)
    identities.update(ac for req, ac in acceptance_relations if req in identities)
    for raw in system.link_nodes(evidence):
        details = []
        target, fragment = system.validate_reference(
            root, source, raw, path=f'{path}:{number}', code='invalid-design-evidence',
            errors=details, require_fragment=True, markdown_target=True,
            follow_symlinks=False, read_target=read_target)
        if target is None:
            errors.append(finding(path, number, 'invalid-design-evidence', '; '.join(details)))
            continue
        relative = target.relative_to(root).as_posix()
        if not system.is_system_design_path(relative):
            errors.append(finding(path, number, 'invalid-design-evidence', f'设计依据须指向专业系统设计条款：{raw}'))
            continue
        matched = False
        for relation in read_design_relations(target):
            if relation.local_fragment != fragment:
                continue
            upstream = read_target(root / relation.upstream_path)
            if relation.upstream_path == path and system.fragment_occurrences(block, relation.upstream_fragment) == 1:
                matched = True
            elif upstream is not None:
                upstream_ids, _product_chain = system.resolved_requirement_context(
                    root, root / relation.upstream_path, upstream, relation.upstream_fragment, read_target, follow_symlinks=False)
                eligible_ids = {identity for identity in identities if relation.upstream_path == path or identity.startswith('O7-REQ-')}
                if eligible_ids.intersection(upstream_ids):
                    matched = True
        if not matched:
            errors.append(finding(path, number, 'design-feature-relation-missing', f'设计条款须有指向本功能或其明确 Trace ID 的需求承接关系：{raw}'))
    return errors


def is_anchor_line(line):
    return re.fullmatch(r"\s*<a\b[^>]*\bid\s*=\s*['\"][^'\"]+['\"][^>]*>\s*(?:</a>)?\s*", line, re.I) is not None


def check_status(path, text, *, root=None, read_target=None, read_design_relations=None):
    if read_design_relations is None and root is not None and read_target is not None:
        read_design_relations = _design_relations_reader(root, read_target)
    lines = product.visible_lines(text)
    starts = []
    for i, (_, line) in enumerate(lines):
        if re.match(r'^ {0,3}#{2,6}\s+(?:REQ-[A-Z0-9_-]+|(?:\d+(?:\.\d+)*[.、]?\s*)?功能(?:点|条目)?[：: ]|功能状态卡)', line, re.I):
            starts.append(i)
    # Explicit cards outside a recognizable feature heading are still checked.
    if not starts and any(re.match(r'^\s*(?:-\s*)?(系统设计状态|实现状态)[:：]', line) for _, line in lines):
        starts = [0]
    errors = []
    for i in starts:
        number, heading = lines[i]
        level_match = re.match(r'^ {0,3}(#{1,6})\s', heading)
        level = len(level_match[1]) if level_match else 0
        end = next((j for j in range(i+1, len(lines)) if (m := re.match(r'^ {0,3}(#{1,6})\s', lines[j][1])) and len(m[1]) <= level), len(lines))
        start = i
        while start > 0 and (not lines[start-1][1].strip() or is_anchor_line(lines[start-1][1])):
            start -= 1
        block_end = end
        if end < len(lines):
            # An anchor immediately before the next heading belongs to that feature.
            while block_end > i + 1 and (not lines[block_end-1][1].strip() or is_anchor_line(lines[block_end-1][1])):
                block_end -= 1
        block = '\n'.join(line for _, line in lines[start:block_end])
        fields = {}
        for label in STATUS_FIELDS:
            matches = re.findall(r'^\s*(?:-\s*)?'+re.escape(label)+r'[:：]\s*(.*?)\s*$', block, re.M)
            if len(matches) > 1:
                errors.append(finding(path, number, 'duplicate-status-field', f'{heading.strip()} 的已声明字段重复：{label}'))
            fields[label] = matches[0].strip('` ') if matches else ''
        if not fields['系统设计状态'] and not fields['实现状态']:
            errors.append(finding(path, number, 'missing-status-field', f'{heading.strip()} 可补充已知状态；未知或未实现不需要填写空证据卡'))
        for label, allowed in [('系统设计状态', DESIGN_STATUS), ('实现状态', IMPLEMENTATION_STATUS)]:
            value = fields[label]
            if value and value not in allowed:
                errors.append(finding(path, number, 'invalid-status', f'{label}={value!r}；允许：{", ".join(sorted(allowed))}'))
        for status, evidence, positive in [('系统设计状态','系统设计依据',{'部分覆盖','已覆盖'}),('实现状态','实现与验证依据',{'部分实现','已实现待验证','已验证'})]:
            if fields[status] in positive and not product.markdown_links(fields[evidence]):
                errors.append(finding(path, number, 'missing-status-evidence', f'{status}必须关联真实证据链接（链接真实性/内容仍需人工核对）'))
        if fields['系统设计状态'] in {'部分覆盖', '已覆盖'} and root is not None and read_target is not None:
            errors.extend(check_design_evidence(path, number, block, fields['系统设计依据'], root, read_target,
                                                read_design_relations=read_design_relations))
        if fields['系统设计状态'] == '不适用':
            values = [re.search(word + r'[:：]\s*([^；;]+)', fields['系统设计依据']) for word in ('理由', '范围')]
            if not all(value and value[1].strip(' `。') not in {'无', '-', 'N/A', '待补', '无独立技术义务'} for value in values):
                errors.append(finding(path, number, 'not-applicable-missing-reason', '设计不适用须说明具体理由和适用范围；需人工核实没有独立技术义务'))
        if fields['系统设计状态'] == '部分覆盖' or fields['实现状态'] in {'部分实现', '已实现待验证'}:
            if not fields['剩余范围'] or fields['剩余范围'] in {'无','N/A','待补','-'}:
                errors.append(finding(path, number, 'partial-missing-scope', '部分状态或待验证声明须明确剩余未覆盖/未验证范围'))
        if fields['实现状态'] == '已验证':
            evidence = fields['实现与验证依据'] + fields['核对日期与基线']
            if not re.search(r'\d{4}-\d{2}-\d{2}', fields['核对日期与基线']) or not all(word in evidence for word in ('基线','环境','验收')):
                errors.append(finding(path, number, 'verified-missing-boundary', '已验证需要核对日期、基线、环境与验收范围'))
    return errors


def newly_broken_status_relations(root, base, head, worktree, selected_paths, read_target, *, read_design_relations=None):
    """Unchanged consumers only pay for newly broken relations, never legacy debt."""
    paths = set(product.run_git(root, 'ls-tree', '-r', '--name-only', head, '--', 'doc/product').splitlines())
    if worktree:
        paths.update(product.run_git(root, 'ls-files', '--others', '--exclude-standard', '--', 'doc/product').splitlines())
    errors = []
    @lru_cache(maxsize=None)
    def base_reader(target):
        return system.committed_target_text(root, base, target)
    base_design_relations = _design_relations_reader(root, base_reader)
    if read_design_relations is None:
        read_design_relations = _design_relations_reader(root, read_target)
    def relation_findings(path, text, reader, design_relations):
        if not text:
            return []
        lifecycle = product.metadata_value(product.document_identity_text(text), '生命周期')
        if lifecycle and lifecycle.strip('` ') in {'superseded', 'retired'}:
            return []
        return [error for error in check_status(path, text, root=root, read_target=reader,
                                                read_design_relations=design_relations)
                if ': invalid-design-evidence:' in error or ': design-feature-relation-missing:' in error]
    for path in sorted(paths - selected_paths):
        if not (path.endswith('.prd.md') or path.endswith('/prd.md')):
            continue
        current = read_target(root / path)
        old = base_reader(root / path)
        old_errors = set(relation_findings(path, old, base_reader, base_design_relations))
        errors.extend(error for error in relation_findings(path, current, read_target, read_design_relations) if error not in old_errors)
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo-root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--base')
    parser.add_argument('--head')
    parser.add_argument('--worktree', action='store_true')
    parser.add_argument('--full-corpus', action='store_true', help='Inventory current product PRD/design and professional system designs; historical findings warn, execution errors fail')
    parser.add_argument('--warn-only', action='store_true')
    args = parser.parse_args()
    root = args.repo_root.resolve()
    if args.full_corpus and (args.base is not None or args.head is not None or args.worktree):
        parser.error('--full-corpus cannot be combined with range/worktree')
    try:
        previous = {}
        if args.full_corpus:
            corpus_head = product.run_git(root, 'rev-parse', '--verify', 'HEAD^{commit}').strip()
            # Validate directory traversal before the product collector can read files.
            product_root = root / 'doc/product'
            for parent in (root / 'doc', product_root):
                if parent.is_symlink():
                    raise OSError(f'symlink-not-allowed: {parent.relative_to(root)}')
            def walk_error(error):
                raise error
            for directory, dirs, _ in (system.os.walk(product_root, followlinks=False, onerror=walk_error) if product_root.exists() else []):
                for name in dirs:
                    path = Path(directory) / name
                    if path.is_symlink():
                        raise OSError(f'symlink-not-allowed: {path.relative_to(root)}')
            documents, hard_errors = product.collect_full_corpus(root)
            if hard_errors:
                print('\n'.join(hard_errors))
                return 2
            @lru_cache(maxsize=None)
            def read_target(target):
                return system.full_corpus_read(root, target)
            selected = [(d.path, d.new_text) for d in documents]
            professional = system.collect_full_corpus(root)
            selected.extend(professional)
            previous.update((path, system.previous_design_text(root, corpus_head, corpus_head, path, include_worktree=True)) for path, _ in professional if system.is_module_design_overview(path))
            roots = sum(system.is_module_design_overview(path) for path, _ in professional)
            root_contracts = sum(system.is_module_design_overview(path) and system.design_requires_traceability(path, text, previous.get(path)) for path, text in professional)
            print(f"doc-core-content: full-corpus selected product={len(documents)} professional={len(professional)} (topic-contracts={len(professional) - roots} module-overviews={roots - root_contracts} module-contracts={root_contracts}); professional designs include all lifecycles")
        else:
            base = product.resolve_commit(root, args.base, 'base')
            head = product.resolve_commit(root, args.head, 'head')
            documents, _ = product.collect_documents(root, base, head, args.worktree)
            selected = [(d.path, d.new_text) for d in documents]
            checkout = product.run_git(root, 'rev-parse', '--verify', 'HEAD^{commit}').strip()
            source_base = product.run_git(root, 'merge-base', base, head).strip()
            read_worktree = system.worktree_text_reader(root, checkout) if args.worktree else None
            @lru_cache(maxsize=None)
            def read_target(target):
                relative = target.relative_to(root).as_posix()
                if args.worktree:
                    # Validate lexical paths and symlinks before any worktree read.
                    system.full_corpus_read(root, target)
                    return read_worktree(relative)
                return system.committed_target_text(root, head, target)
            system_paths = system.changed_system_design_paths(
                root, base, head, args.worktree, worktree_reader=read_worktree)
            rename_origins = system.design_rename_origins(root, source_base, head, args.worktree) if system_paths else None
            for path in system_paths:
                relative = path.relative_to(root).as_posix()
                text = read_target(root / relative)
                if text is None:
                    raise OSError(f'selected document cannot be read: {relative}')
                selected.append((relative, text))
                previous[relative] = system.previous_design_text(root, source_base, head, relative,
                    include_worktree=args.worktree, rename_origins=rename_origins)
        read_design_relations = _design_relations_reader(root, read_target)
        errors = []
        if not args.full_corpus:
            errors.extend(newly_broken_status_relations(root, source_base, head, args.worktree,
                {path for path, _ in selected}, read_target, read_design_relations=read_design_relations))
        for path, text in selected:
            lifecycle = product.metadata_value(product.document_identity_text(text), '生命周期')
            if path.startswith('doc/product/') and lifecycle and lifecycle.strip('` ') in {'superseded', 'retired'}:
                print(f'doc-core-content: skipped active core/status rules: {path}: lifecycle={lifecycle}; identity and successor remain checked by product content checker')
                continue
            core_findings = check_core(path, text, previous.get(path))
            errors.extend(core_findings)
            if not core_findings and any('结构裁剪' in line for _, line in product.visible_lines(text)):
                print(f'::notice::doc-core-content: {path}: 结构裁剪映射可定位；理由、适用范围和风险仍需人工复核')
            if path.startswith('doc/product/') and (path.endswith('.prd.md') or path.endswith('/prd.md')):
                errors.extend(check_status(path, text, root=root, read_target=read_target,
                                          read_design_relations=read_design_relations))
        print(f'doc-core-content: checked {len(selected)} documents; selected documents scanned in full, including legacy REQ blocks; unrecognized feature prose and evidence truth require human review')
        if args.full_corpus:
            return report_findings(errors, True) if errors else 0
        return report_classified_findings(errors, ADVISORY_CODES) if errors else 0
    except (OSError, ValueError) as exc:
        print(f'doc-core-content: error: {exc}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
