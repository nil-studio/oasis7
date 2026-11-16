#!/usr/bin/env python3
import importlib.util
import contextlib
import io
from pathlib import Path
import subprocess
import sys
import unittest
import os
import tempfile
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('doc_core', HERE / 'doc-core-content-check.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class ContentTests(unittest.TestCase):
    def test_bad_structure(self):
        self.assertTrue(module.check_core('doc/product/a/x.prd.md', '# Empty'))
    def test_valid_structure(self):
        text = '\n'.join('## '+x for x in ['文档身份','玩家问题与目标','范围与 Non-Goals','玩家体验与决策','产品要求','验收与证据','设计取舍与未决问题'])
        self.assertEqual(module.check_core('doc/product/a/x.prd.md', text), [])
    def test_partial_card(self):
        text = '### REQ-DEMO-001\n- 系统设计状态：部分覆盖\n- 实现状态：部分实现\n'
        findings = module.check_status('a.prd.md', text)
        self.assertTrue(any('missing-status-evidence' in x for x in findings))
        self.assertTrue(any('partial-missing-scope' in x for x in findings))
    def test_unknown_is_honest(self):
        text = '### REQ-DEMO-001\n' + '\n'.join('- '+k+'：'+v for k,v in [('系统设计状态','未核实'),('实现状态','未核实'),('系统设计依据','尚未核查'),('实现与验证依据','尚未核查'),('核对日期与基线','尚未核查'),('剩余范围','整个功能待核查')])
        self.assertEqual(module.check_status('a.prd.md', text), [])
        for design, implementation in [('未核实', '未核实'), ('未设计', '未实现')]:
            minimal = f'### REQ-DEMO-001\n- 系统设计状态：{design}\n- 实现状态：{implementation}\n'
            self.assertEqual(module.check_status('a.prd.md', minimal), [])
    def test_positive_claims_keep_their_evidence_and_boundaries(self):
        for design in ('部分覆盖', '已覆盖'):
            text = f'### REQ-DEMO-001\n- 系统设计状态：{design}\n'
            self.assertTrue(any('missing-status-evidence' in error for error in module.check_status('a.prd.md', text)))
        pending = '### REQ-DEMO-001\n- 实现状态：已实现待验证\n- 实现与验证依据：[交付](https://github.com/nil-studio/oasis7/pull/5770)\n'
        self.assertTrue(any('partial-missing-scope' in error for error in module.check_status('a.prd.md', pending)))
        self.assertEqual(module.check_status('a.prd.md', pending + '- 剩余范围：真实环境验收尚未执行\n'), [])
        duplicate = '### REQ-DEMO-001\n- 实现状态：未实现\n- 实现状态：已验证\n'
        self.assertTrue(any('duplicate-status-field' in error for error in module.check_status('a.prd.md', duplicate)))
    def test_illegal_enum(self):
        self.assertTrue(any('invalid-status' in x for x in module.check_status('a.prd.md', '### REQ-X-1\n- 实现状态：merged')))
    def test_verified_needs_evidence(self):
        self.assertTrue(any('missing-status-evidence' in x for x in module.check_status('a.prd.md', '### REQ-X-1\n- 实现状态：已验证')))
    def test_comment_and_code_do_not_count(self):
        self.assertEqual(module.check_status('a.prd.md', '```\n### REQ-X-1\n```'), [])
    def test_cropping_is_not_silent_pass(self):
        self.assertTrue(any('invalid-structure-tailoring' in x for x in module.check_core('doc/system/x.design.md','- 结构裁剪：理由：小改；范围：接口；风险：低；映射：现有接口节')))
    def test_not_applicable_requires_reason_and_scope(self):
        errors = module.check_status('a.prd.md', '### REQ-X-1\n- 系统设计状态：不适用')
        self.assertTrue(any('not-applicable-missing-reason' in x for x in errors))
        self.assertFalse(any('invalid-status' in x for x in errors))
    def test_invalid_invocation(self):
        p = subprocess.run([sys.executable, str(HERE/'doc-core-content-check.py'), '--warn-only','--base','bad'],capture_output=True,text=True)
        self.assertNotEqual(p.returncode,0)


class FeatureRelationTests(unittest.TestCase):
    PATH = 'doc/product/demo.prd.md'
    DESIGN = 'doc/system/demo.design.md'

    def setUp(self):
        self.root = Path('/fixture')
        self.docs = {}

    def card(self, status='已覆盖', evidence='[条款](../system/demo.design.md#design-one)', heading='REQ-DEMO-001'):
        return ('### ' + heading + '\n' + '\n'.join('- '+key+'：'+value for key, value in [
            ('系统设计状态', status), ('实现状态', '部分实现'), ('系统设计依据', evidence),
            ('实现与验证依据', '[代码](../../src/demo.py)'), ('核对日期与基线', '尚未核查'), ('剩余范围', '实现尚缺拒绝路径')]))

    def design(self, upstream='../product/demo.prd.md#req-demo-001', local='design-one'):
        return ('### 2.1 需求承接与分配表\n'
                '| 上游 requirement | 具体 obligation | 本设计条款 | 外部 owner | 明确排除 |\n'
                '| --- | --- | --- | --- | --- |\n'
                f'| [功能]({upstream}) | 结果与条件 | [条款](#{local}) | engineer | 无 |\n'
                f'### {local}\n条款正文\n')

    def check(self, text):
        self.docs[self.PATH] = text
        return module.check_status(self.PATH, text, root=self.root,
            read_target=lambda p: self.docs.get(p.relative_to(self.root).as_posix()))

    def test_real_bidirectional_design_and_implementation_gap(self):
        self.docs[self.DESIGN] = self.design()
        self.assertEqual(self.check(self.card()), [])

    def test_positive_design_evidence_accepts_single_flexible_mapping(self):
        self.docs[self.DESIGN] = ('# Design\n## Decision and verification\n'
            '| 验证方法 | 上游要求 | 本设计条款 |\n| --- | --- | --- |\n'
            '| Check normal and rejected input | [REQ-DEMO-001](../product/demo.prd.md#req-demo-001) | [design](#design-one) |\n'
            '## Design-one\nImplementation boundary.\n')
        self.assertEqual(self.check(self.card()), [])
        self.docs[self.DESIGN] = self.docs[self.DESIGN].replace('#req-demo-001)', '#req-other-001)')
        self.assertTrue(any('design-feature-relation-missing' in error for error in self.check(self.card() + '\n### REQ-OTHER-001\nAnother feature.\n')))

    def test_fake_external_product_missing_fragment_and_unrelated_design(self):
        self.docs[self.DESIGN] = self.design()
        for link in ('https://example.com/design#one', '../system/missing.design.md#one',
                     '../system/demo.design.md#missing', 'demo.prd.md#req-demo-001', '../system/demo.design.md'):
            with self.subTest(link=link):
                self.assertTrue(any('invalid-design-evidence' in error for error in self.check(self.card(evidence=f'[设计]({link})'))))
        self.docs[self.DESIGN] = self.design(upstream='../product/demo.prd.md#req-other-001')
        text = self.card() + '\n### REQ-OTHER-001\n说明'
        self.assertTrue(any('design-feature-relation-missing' in error for error in self.check(text)))

    def test_many_designs_for_one_feature(self):
        self.docs[self.DESIGN] = self.design()
        self.docs['doc/system/second.design.md'] = self.design(local='design-two')
        text = self.card(evidence='[甲](../system/demo.design.md#design-one)；[乙](../system/second.design.md#design-two)')
        self.assertEqual(self.check(text), [])
        self.docs.pop('doc/system/second.design.md')
        self.assertTrue(any('invalid-design-evidence' in error for error in self.check(text)))

    def test_many_features_share_design(self):
        self.docs[self.DESIGN] = self.design().replace('### design-one',
            '| [另一功能](../product/demo.prd.md#req-demo-002) | 结果与条件 | [条款](#design-one) | engineer | 无 |\n### design-one')
        self.assertEqual(self.check(self.card() + '\n' + self.card(heading='REQ-DEMO-002')), [])

    def test_global_trace_identity_can_bridge_documents_but_local_id_cannot(self):
        trace = 'O7-REQ-123e4567-e89b-42d3-a456-426614174000'
        self.docs[self.DESIGN] = self.design(upstream='../product/global.prd.md#global-requirement')
        self.docs['doc/product/global.prd.md'] = '## Global requirement\n- Trace ID: ' + trace
        self.assertEqual(self.check(self.card() + '\n- Trace ID: ' + trace), [])
        self.docs[self.DESIGN] = self.design(upstream='../product/global.prd.md#req-demo-001')
        self.docs['doc/product/global.prd.md'] = '## REQ-DEMO-001\nOther feature'
        self.assertTrue(any('design-feature-relation-missing' in error for error in self.check(self.card())))

    def test_feature_acceptance_relation_is_explicit_and_local(self):
        self.docs[self.DESIGN] = self.design(upstream='../product/demo.prd.md#ac-demo-001')
        acceptance = '\n### AC-DEMO-001\n- 关联要求：REQ-DEMO-001\n验收说明'
        self.assertEqual(self.check(self.card() + acceptance), [])
        self.assertTrue(any('design-feature-relation-missing' in error for error in self.check(self.card() + acceptance.replace('关联要求：REQ-DEMO-001', '关联要求：REQ-OTHER-001'))))

    def test_next_feature_anchor_does_not_leak_into_previous_card(self):
        self.docs[self.DESIGN] = self.design(upstream='../product/demo.prd.md#another-feature')
        text = self.card() + '\n\n<a class="stable" id = "another-feature"></a>\n### AC-OTHER-001\n- 关联要求：REQ-OTHER-001'
        self.assertTrue(any('design-feature-relation-missing' in error for error in self.check(text)))
        self.docs[self.DESIGN] = self.design(upstream='../product/demo.prd.md#ac-stable')
        text = self.card() + '\n\n<a id="ac-stable"></a>\n### AC-DEMO-001\n- 关联要求：REQ-DEMO-001'
        self.assertEqual(self.check(text), [])

    def test_trace_refs_use_single_declared_global_identity(self):
        trace = 'O7-REQ-123e4567-e89b-42d3-a456-426614174000'
        self.docs[self.DESIGN] = self.design(upstream='../product/global.prd.md#global-requirement')
        self.docs['doc/product/global.prd.md'] = '## Global requirement\n- Trace ID: ' + trace
        self.assertEqual(self.check(self.card() + '\n- Trace refs: ' + trace), [])

    def test_indirect_trace_reference_chain_requires_the_same_declared_identity(self):
        trace = 'O7-REQ-123e4567-e89b-42d3-a456-426614174000'
        self.docs[self.DESIGN] = self.design(upstream='../rules/runtime.prd.md#sc-runtime-001')
        self.docs['doc/rules/runtime.prd.md'] = ('## SC-RUNTIME-001\n- Trace refs: ' + trace +
            '\n上游：[原始功能](../product/demo.prd.md#req-demo-001)\n')
        self.assertEqual(self.check(self.card() + '\n- Trace ID: ' + trace), [])
        self.docs['doc/rules/runtime.prd.md'] = self.docs['doc/rules/runtime.prd.md'].replace(trace, 'O7-REQ-123e4567-e89b-42d3-a456-426614174001')
        self.assertTrue(any('design-feature-relation-missing' in error for error in self.check(self.card() + '\n- Trace ID: ' + trace)))

    def test_table_trace_declaration_and_reference_are_supported(self):
        trace = 'O7-REQ-123e4567-e89b-42d3-a456-426614174000'
        self.docs[self.DESIGN] = self.design(upstream='../product/global.prd.md#global-requirement')
        self.docs['doc/product/global.prd.md'] = ('## Global requirement\n| Trace ID | 说明 |\n| --- | --- |\n| ' + trace + ' | 功能 |\n')
        self.assertEqual(self.check(self.card() + '\n| Trace refs | 说明 |\n| --- | --- |\n| ' + trace + ' | 本功能 |\n'), [])

    def test_not_applicable_placeholders_are_rejected(self):
        self.assertTrue(any('not-applicable-missing-reason' in error for error in self.check(self.card(status='不适用', evidence='理由：无；范围：无'))))

    def test_partial_and_undesigned_are_legal(self):
        self.docs[self.DESIGN] = self.design()
        self.assertEqual(self.check(self.card(status='部分覆盖')), [])
        self.assertEqual(self.check(self.card(status='未设计', evidence='尚未设计')), [])

    def test_explicit_anchor_before_feature_heading(self):
        self.docs[self.DESIGN] = self.design(upstream='../product/demo.prd.md#feature-stable')
        self.assertEqual(self.check('<a id="feature-stable"></a>\n' + self.card()), [])

    def test_invalid_demand_row_cannot_be_cached_as_feature_evidence(self):
        self.docs[self.DESIGN] = self.design().replace('[功能]', '[REQ-OTHER-001]')
        errors = self.check(self.card())
        self.assertEqual(sum(': design-feature-relation-missing:' in error for error in errors), 1)
        self.docs[self.DESIGN] = self.design()
        self.assertEqual(self.check(self.card()), [])

    def test_later_row_read_failure_is_not_hidden_by_an_earlier_match(self):
        self.docs[self.PATH] = self.card()
        self.docs[self.DESIGN] = self.design().replace('### design-one',
            '| [后续功能](../product/unreadable.prd.md#req-demo-002) | 条件 | [条款](#design-one) | engineer | 无 |\n### design-one')
        def reader(path):
            relative = path.relative_to(self.root).as_posix()
            if relative == 'doc/product/unreadable.prd.md':
                raise PermissionError('unreadable relation target')
            return self.docs.get(relative)
        with self.assertRaisesRegex(PermissionError, 'unreadable relation target'):
            module.check_status(self.PATH, self.docs[self.PATH], root=self.root, read_target=reader)


class FullCorpusTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'fixture@example.invalid')
        self.git('config', 'user.name', 'Fixture')
        self.put('seed', 'fixture')
        self.commit()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True).strip()

    def put(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
        return target

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')
        return self.git('rev-parse', 'HEAD')

    def run_checker(self, name, *args):
        return subprocess.run([sys.executable, str(HERE / (name + '.py')), '--repo-root', str(self.root), *args], capture_output=True, text=True)

    def run_main(self, *args):
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(sys, 'argv', [str(HERE / 'doc-core-content-check.py'), '--repo-root', str(self.root), *args]), \
             contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
            result = module.main()
        return result, output.getvalue() + errors.getvalue()

    def test_shared_design_rows_are_bounded_and_last_feature_still_checks(self):
        fixture = FeatureRelationTests()
        count = 12
        text = '\n\n'.join(fixture.card(heading=f'REQ-DEMO-{index:03d}') for index in range(1, count + 1))
        extra_rows = ''.join(
            f'| [功能](../product/demo.prd.md#req-demo-{index:03d}) | 条件 | [条款](#design-one) | engineer | 无 |\n'
            for index in range(2, count + 1))
        design = fixture.design().replace('### design-one', extra_rows + '### design-one')
        self.put(fixture.PATH, text)
        self.put(fixture.DESIGN, design)
        base = self.commit()
        with patch.object(module.system, 'relation_from_demand_row', wraps=module.system.relation_from_demand_row) as rows:
            result, output = self.run_main('--base', base, '--head', base, '--worktree')
        self.assertEqual(result, 0, output)
        self.assertLessEqual(rows.call_count, count * 2, 'baseline and current rows must not scale with feature-card count')

        # The base is valid at this exact path. The current last row now points
        # at the first feature, so sharing rows must not share feature decisions
        # or let the baseline's valid relation hide the current breakage.
        self.put(fixture.DESIGN, design.replace(f'#req-demo-{count:03d}', '#req-demo-001'))
        head = self.commit()
        with patch.object(module.system, 'relation_from_demand_row', wraps=module.system.relation_from_demand_row) as rows:
            result, output = self.run_main('--base', base, '--head', head, '--worktree')
        self.assertEqual(result, 1, output)
        self.assertEqual(output.count(': design-feature-relation-missing:'), 1, output)
        last_line = text[:text.index(f'### REQ-DEMO-{count:03d}')].count('\n') + 1
        self.assertIn(f'{fixture.PATH}:{last_line}:', output)
        self.assertLessEqual(rows.call_count, count * 2)

    def test_reference_targets_share_one_worktree_status_snapshot(self):
        fixture = FeatureRelationTests()
        cards = []
        for index in range(1, 13):
            design_path = f'doc/system/consumer-{index}.design.md'
            cards.append(fixture.card(heading=f'REQ-DEMO-{index:03d}',
                evidence=f'[条款](../system/consumer-{index}.design.md#design-one)'))
            self.put(design_path, fixture.design(upstream=f'../product/demo.prd.md#req-demo-{index:03d}'))
        self.put(fixture.PATH, '\n\n'.join(cards))
        base = self.commit()
        with patch.object(module.system, 'run_git', wraps=module.system.run_git) as git_calls:
            result, output = self.run_main('--base', base, '--head', base, '--worktree')
        self.assertEqual(result, 0, output)
        scans = [call.args[1:] for call in git_calls.call_args_list if call.args[1] in {'diff', 'ls-files'}]
        self.assertLessEqual(len(scans), 16, f'twelve real reference targets repeated repository scans: {scans}')

    def test_invocation_cache_keeps_staged_dirty_and_deleted_relations_fresh(self):
        fixture = FeatureRelationTests()
        self.put(fixture.PATH, fixture.card())
        self.put(fixture.DESIGN, fixture.design())
        base = self.commit()
        args = ('--base', base, '--head', base, '--worktree')
        result, output = self.run_main(*args)
        self.assertEqual(result, 0, output)

        self.put(fixture.DESIGN, fixture.design(upstream='../product/demo.prd.md#missing'))
        self.git('add', fixture.DESIGN)
        self.git('restore', '--source=HEAD', '--worktree', '--', fixture.DESIGN)
        result, output = self.run_main(*args)
        self.assertEqual(result, 1, output)
        self.assertIn(': design-feature-relation-missing:', output)

        self.git('reset', '-q', 'HEAD', '--', fixture.DESIGN)
        result, output = self.run_main(*args)
        self.assertEqual(result, 0, output)
        self.put(fixture.DESIGN, fixture.design(upstream='../product/demo.prd.md#missing'))
        result, output = self.run_main(*args)
        self.assertEqual(result, 1, output)
        self.assertIn(': design-feature-relation-missing:', output)

        (self.root / fixture.DESIGN).unlink()
        result, output = self.run_main(*args)
        self.assertEqual(result, 1, output)
        self.assertIn(': invalid-design-evidence:', output)

    def test_new_invocation_rejects_unsafe_target_even_after_cached_success(self):
        fixture = FeatureRelationTests()
        self.put(fixture.PATH, fixture.card())
        target = self.put(fixture.DESIGN, fixture.design())
        base = self.commit()
        args = ('--base', base, '--head', base, '--worktree', '--warn-only')
        result, output = self.run_main(*args)
        self.assertEqual(result, 0, output)
        with tempfile.TemporaryDirectory() as outside:
            foreign = Path(outside) / 'foreign.design.md'
            foreign.write_text(fixture.design())
            target.unlink()
            target.symlink_to(foreign)
            result, output = self.run_main(*args)
        self.assertEqual(result, 2, output)
        self.assertIn('unsafe corpus target', output)

    def test_full_and_scoped_missing_structure_are_advisory(self):
        self.put('doc/system/legacy.design.md', '# Legacy')
        base = self.commit()
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            scoped = self.run_checker(name, '--base', base, '--head', base)
            self.assertEqual(scoped.returncode, 0, scoped.stdout + scoped.stderr)
            full = self.run_checker(name, '--full-corpus')
            self.assertEqual(full.returncode, 0, full.stdout + full.stderr)
            self.assertIn('::warning::', full.stdout)
            self.assertIn('doc/system/legacy.design.md', full.stdout)
        self.put('doc/system/legacy.design.md', '# Substantive change')
        head = self.commit()
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            strict = self.run_checker(name, '--base', base, '--head', head)
            self.assertEqual(strict.returncode, 0, strict.stdout + strict.stderr)
            self.assertIn('::warning::', strict.stdout)

    def test_unchanged_positive_card_newly_broken_design_is_strict(self):
        fixture = FeatureRelationTests()
        self.put(fixture.PATH, fixture.card())
        self.put(fixture.DESIGN, fixture.design())
        base = self.commit()
        (self.root / fixture.DESIGN).unlink()
        head = self.commit()
        p = self.run_checker('doc-core-content-check', '--base', base, '--head', head)
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn('invalid-design-evidence', p.stdout)
        full = self.run_checker('doc-core-content-check', '--full-corpus')
        self.assertEqual(full.returncode, 0, full.stdout + full.stderr)
        self.assertIn('invalid-design-evidence', full.stdout)
        self.put('seed', 'unrelated change')
        next_head = self.commit()
        p = self.run_checker('doc-core-content-check', '--base', head, '--head', next_head)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_target_snapshot_not_checked_out_future_and_worktree_detects_deletion(self):
        fixture = FeatureRelationTests()
        self.put(fixture.PATH, fixture.card())
        self.put(fixture.DESIGN, fixture.design())
        base = self.commit()
        (self.root / fixture.DESIGN).unlink()
        p = self.run_checker('doc-core-content-check', '--base', base, '--head', base)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = self.run_checker('doc-core-content-check', '--base', base, '--head', base, '--worktree')
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertIn('invalid-design-evidence', p.stdout)

    def test_inventory_excludes_product_fiction_overlays_but_keeps_retired(self):
        self.put('doc/product/a.design.md', '# Product')
        self.put('doc/novel/a.design.md', '# Novel')
        self.put('site/story/a.design.md', '# Story')
        self.put('doc/engineering/doc-governance/system-design-writing-standard.design.md', '# Norm')
        self.put('doc/system/retired.design.md', '# Retired\n## 文档身份\n- 生命周期：retired')
        docs = module.system.collect_full_corpus(self.root)
        self.assertEqual([p for p, _ in docs], ['doc/system/retired.design.md'])
        full = self.run_checker('doc-core-content-check', '--full-corpus')
        self.assertEqual(full.returncode, 0, full.stdout + full.stderr)
        self.assertIn('product=1 professional=1', full.stdout)
        self.assertIn('doc/system/retired.design.md', full.stdout)

    def test_conflicting_arguments_fail_even_warn_only(self):
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            for conflict in (('--base', 'bad'), ('--head', 'bad'), ('--base', ''), ('--head', ''), ('--worktree',)):
                p = self.run_checker(name, '--full-corpus', '--warn-only', *conflict)
                self.assertEqual(p.returncode, 2)

    def test_symlink_source_and_parent_hard_fail(self):
        target = self.put('outside/design', '# hidden')
        link = self.root / 'doc/system/link.design.md'
        link.parent.mkdir(parents=True)
        link.symlink_to(target)
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            p = self.run_checker(name, '--full-corpus', '--warn-only')
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        link.unlink()
        (self.root / 'doc/system/linked').symlink_to(target.parent, target_is_directory=True)
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            p = self.run_checker(name, '--full-corpus')
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)

    def test_product_parent_symlink_hard_fail_before_read(self):
        outside = self.root / 'external'
        outside.mkdir()
        (self.root / 'doc').mkdir()
        (self.root / 'doc/product').symlink_to(outside, target_is_directory=True)
        p = self.run_checker('doc-core-content-check', '--full-corpus')
        self.assertEqual(p.returncode, 2)

    def test_read_and_walk_errors_are_not_findings(self):
        self.put('doc/system/a.design.md', '# Legacy')
        with patch.object(Path, 'read_text', side_effect=PermissionError('denied')):
            with self.assertRaises(PermissionError):
                module.system.collect_full_corpus(self.root)
        def broken_walk(*args, **kwargs):
            kwargs['onerror'](PermissionError('walk denied'))
            return iter(())
        with patch.object(module.system.os, 'walk', broken_walk):
            with self.assertRaises(PermissionError):
                module.system.collect_full_corpus(self.root)
        (self.root / 'doc/system/a.design.md').write_bytes(b'\xff')
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            p = self.run_checker(name, '--full-corpus', '--warn-only')
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)

    def test_full_traceability_rejects_unsafe_reference(self):
        spec = importlib.util.spec_from_file_location('trace_fixture', HERE / 'system-design-traceability-check.test.py')
        fixture = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fixture)
        self.put(fixture.DESIGN, fixture.DESIGN_HEADER)
        self.put(fixture.PRODUCT, fixture.PRODUCT_TEXT)
        with tempfile.TemporaryDirectory() as outside:
            foreign = Path(outside) / 'sample.test.py'
            foreign.write_text('secret')
            target = self.root / fixture.TEST_SOURCE
            target.parent.mkdir(parents=True)
            target.symlink_to(foreign)
            p = self.run_checker('system-design-traceability-check', '--full-corpus', '--warn-only')
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
            self.assertIn('unsafe corpus target', p.stderr)

    def test_module_roots_inventory_and_role_specific_rules(self):
        self.put('doc/system/design.md', '# Module overview')
        self.put('doc/product/design.md', '# Product root')
        self.put('doc/novel/design.md', '# Novel root')
        self.put('doc/system/nested/design.md', '# Not a module root')
        self.put('doc/system/topic.design.md', '# Legacy topic')
        head = self.commit()
        docs = module.system.collect_full_corpus(self.root)
        self.assertEqual([p for p, _ in docs], ['doc/system/design.md', 'doc/system/topic.design.md'])
        self.assertTrue(module.system.is_system_design_path('doc/system/design.md'))
        p = self.run_checker('system-design-traceability-check', '--full-corpus')
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn('topic-contracts=1 module-overviews=1', p.stdout)
        self.assertIn('module-overview: doc/system/design.md', p.stdout)
        self.assertNotIn('trace-upstream-missing: doc/system/design.md', p.stdout)
        self.assertIn('trace-mapping-format: doc/system/topic.design.md', p.stdout)
        p = self.run_checker('doc-core-content-check', '--full-corpus')
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn('topic-contracts=1 module-overviews=1', p.stdout)
        self.assertIn('missing-core-section: doc/system/design.md', p.stdout)
        headings = '\n'.join('## ' + heading for heading in ('架构总览', '组件职责', '主链路', '数据流与状态流', '专题导航'))
        self.assertEqual(module.check_core('doc/system/design.md', headings), [])
        self.put('doc/system/design.md', '# Changed module root')
        new_head = self.commit()
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            p = self.run_checker(name, '--base', head, '--head', new_head)
            self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_renamed_root_retains_inventory_but_headings_are_advisory(self):
        old_path, new_path = 'doc/system/design.md', 'doc/runtime/design.md'
        overview = '\n'.join('## ' + heading + '\n' + ('保留模块架构正文与职责描述。' * 12)
            for heading in ('架构总览', '组件职责', '主链路', '数据流与状态流', '专题导航'))
        self.put(old_path, overview + '\n### 2.1 需求承接与分配表\n### 11.1 验证映射表\n')
        base = self.commit()
        self.put(new_path, overview)
        (self.root / old_path).unlink()
        self.git('add', '.')
        self.assertRegex(self.git('diff', '--cached', '--name-status', '-M', base), r'R\d+\s+doc/system/design.md\s+doc/runtime/design.md')
        p = self.run_checker('doc-core-content-check', '--base', base, '--head', base, '--worktree')
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn('missing-core-section: ' + new_path, p.stdout)
        full = self.run_checker('doc-core-content-check', '--full-corpus')
        self.assertEqual(full.returncode, 0, full.stdout + full.stderr)
        self.assertIn('module-contracts=1', full.stdout)
        self.assertIn('missing-core-section: ' + new_path, full.stdout)
        head = self.commit()
        p = self.run_checker('doc-core-content-check', '--base', base, '--head', head)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn('missing-core-section: ' + new_path, p.stdout)

    def test_root_with_empty_mapping_heading_gets_complete_contract(self):
        self.put('doc/system/design.md', '# Root contract\n### 2.1 需求承接与分配表\n')
        p = self.run_checker('system-design-traceability-check', '--full-corpus')
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertIn('module-overviews=0 module-contracts=1', p.stdout)
        self.assertIn('trace-mapping-format: doc/system/design.md', p.stdout)
        self.assertIn('trace-validation-format: doc/system/design.md', p.stdout)
        self.assertFalse(module.system.has_traceability_heading('```\n### 2.1 需求承接与分配表\n```'))
        self.assertFalse(module.system.has_traceability_heading('<!--\n### 11.1 验证映射表\n-->'))
        self.assertTrue(module.system.has_traceability_heading('### 11.1 验证映射表'))

    def test_missing_dependency_is_hard_error(self):
        for name in ('system-design-traceability-check', 'doc-core-content-check'):
            p = subprocess.run([sys.executable, '-S', str(HERE / (name + '.py')), '--repo-root', str(self.root), '--full-corpus', '--warn-only'], capture_output=True, text=True, env={**os.environ, 'PYTHONPATH': ''})
            self.assertNotEqual(p.returncode, 0)
            self.assertRegex(p.stderr + p.stdout, 'markdown[-_]it')

    def test_reference_escape_is_hard_error_but_missing_is_finding(self):
        with tempfile.TemporaryDirectory() as outside:
            foreign = Path(outside) / 'secret'
            foreign.write_text('outside')
            link = self.root / 'link'
            link.symlink_to(foreign)
            with self.assertRaises(OSError):
                module.system.full_corpus_read(self.root, link)
        self.assertIsNone(module.system.full_corpus_read(self.root, self.root / 'absent'))
        with self.assertRaises(OSError):
            module.system.full_corpus_read(self.root, self.root / 'absent', required=True)

if __name__ == '__main__': unittest.main()
