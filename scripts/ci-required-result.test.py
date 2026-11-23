#!/usr/bin/env python3
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('result', Path(__file__).with_name('ci-required-result.py'))
result = importlib.util.module_from_spec(spec)
spec.loader.exec_module(result)


class Results(unittest.TestCase):
    def setUp(self):
        self.jobs = {'baseline': 'baseline', 'rust': 'rust', 'windows': 'windows'}
        self.plan = {'groups': ['baseline', 'windows']}
        self.needs = {'select': {'result': 'success'}, 'baseline': {'result': 'success'},
                      'rust': {'result': 'skipped'}, 'windows': {'result': 'success'}}

    def test_command_center_failure_cannot_pass_gate(self):
        jobs = {'baseline': 'baseline', 'command_center': 'command-center'}
        plan = {'groups': ['baseline', 'command_center']}
        for state in ('failure', 'cancelled', 'skipped', None):
            with self.subTest(result=state), self.assertRaises(ValueError):
                result.validate(plan, {'select': {'result': 'success'},
                                          'baseline': {'result': 'success'},
                                          'command-center': {'result': state}}, jobs)

    def test_expected_success(self):
        self.assertTrue(result.validate(self.plan, self.needs, self.jobs))
        self.needs['rust']['result'] = 'success'
        self.assertTrue(result.validate(self.plan, self.needs, self.jobs))

    def test_selected_failure_cancelled_missing_skipped_unknown(self):
        for value in ['failure', 'cancelled', 'skipped', 'unknown', None]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.needs['windows'] = {'result': value}
                result.validate(self.plan, self.needs, self.jobs)
        del self.needs['windows']
        with self.assertRaises(ValueError):
            result.validate(self.plan, self.needs, self.jobs)

    def test_selected_workspace_support_failure_or_missing_result_fails_gate(self):
        jobs = {'baseline': 'baseline', 'workspace_support': 'workspace-support'}
        plan = {'groups': ['baseline', 'workspace_support']}
        needs = {'select': {'result': 'success'}, 'baseline': {'result': 'success'},
                 'workspace-support': {'result': 'success'}}
        self.assertTrue(result.validate(plan, needs, jobs))
        for value in ('failure', 'cancelled', 'skipped', 'unknown', None):
            with self.subTest(value=value), self.assertRaises(ValueError):
                needs['workspace-support'] = {'result': value}
                result.validate(plan, needs, jobs)
        del needs['workspace-support']
        with self.assertRaises(ValueError):
            result.validate(plan, needs, jobs)

    def test_select_failure_or_skip(self):
        for value in ['failure', 'cancelled', 'skipped', None]:
            self.needs['select'] = {'result': value}
            with self.assertRaises(ValueError):
                result.validate(self.plan, self.needs, self.jobs)

    def test_unselected_failure_is_not_hidden(self):
        for value in ['failure', 'cancelled', 'unknown', None]:
            self.needs['rust'] = {'result': value}
            with self.assertRaises(ValueError):
                result.validate(self.plan, self.needs, self.jobs)

    def test_added_workflow_dependency_must_explicitly_succeed(self):
        # Adding another gate dependency must never hide its failed or absent result.
        invalid = [{'result': value} for value in ('failure', 'cancelled', 'skipped', 'unknown', None)]
        invalid.extend([{}, {'result': 1}, None, [], 'success', True])
        for entry in invalid:
            with self.subTest(entry=entry), self.assertRaises(ValueError):
                self.needs['new-required-dependency'] = entry
                result.validate(self.plan, self.needs, self.jobs)
        self.needs['new-required-dependency'] = {'result': 'success'}
        self.assertTrue(result.validate(self.plan, self.needs, self.jobs))
        self.assertEqual(self.needs['rust']['result'], 'skipped')

    def test_invalid_groups_fail_closed(self):
        for groups in (None, {}, 'baseline', [], ['baseline', 'unknown'],
                       ['baseline', 'baseline'], ['windows'], ['baseline', {}]):
            with self.subTest(groups=groups), self.assertRaises(ValueError):
                result.validate({'groups': groups}, self.needs, self.jobs)

    def test_extra_legacy_plan_fields_do_not_change_results(self):
        self.plan['matrices'] = {'legacy': 'unused'}
        self.assertTrue(result.validate(self.plan, self.needs, self.jobs))

    def full_fixture(self, *, candidate=False, main_dispatch=False):
        workflow = 'a' * 40
        source = 'b' * 40 if candidate else workflow
        plan = {'scope': 'explicit-full', 'groups': list(self.jobs),
                'execution_profile': 'candidate-full' if candidate or main_dispatch else 'periodic-full',
                'check_inventory_version': 1, 'source_sha': source,
                'tested_sha': source, 'policy_sha': workflow}
        needs = {'select': {'result': 'success'},
                 **{job: {'result': 'success'} for job in self.jobs.values()},
                 'oasis7-test-build': {'result': 'success'},
                 'full-regression': {'result': 'skipped' if candidate else 'success'},
                 'candidate-full-regression': {'result': 'success' if candidate else 'skipped'}}
        args = {'event_name': 'workflow_dispatch' if candidate or main_dispatch else 'schedule',
                'ref': 'refs/heads/main', 'workflow_sha': workflow, 'source_sha': source,
                'run_mode': 'full' if candidate or main_dispatch else ''}
        return plan, needs, args

    def test_full_main_and_exact_manual_candidate(self):
        for candidate, dispatch in ((False, False), (False, True), (True, True)):
            plan, needs, args = self.full_fixture(candidate=candidate, main_dispatch=dispatch)
            self.assertTrue(result.validate_full(plan, needs, self.jobs, **args))
        args['ref'] = 'refs/heads/candidate'
        plan['bootstrap_full'] = True
        self.assertTrue(result.validate_full(plan, needs, self.jobs, **args))

    def test_formal_deferred_fast_scope_full_does_not_weaken_explicit_full_gate(self):
        self.jobs['formal_process'] = 'formal-process'
        for candidate in (False, True):
            full, needs, args = self.full_fixture(candidate=candidate)
            full['formal_full_profiles_only_version'] = 1
            self.assertTrue(result.validate_full(full, needs, self.jobs, **args))
            missing = copy.deepcopy(full)
            missing['groups'].remove('formal_process')
            with self.assertRaisesRegex(ValueError, 'omitted a configured group'):
                result.validate_full(missing, needs, self.jobs, **args)
            for status in ('failure', 'cancelled', 'skipped', None):
                failed = copy.deepcopy(needs)
                failed['formal-process']['result'] = status
                with self.subTest(candidate=candidate, status=status), self.assertRaises(ValueError):
                    result.validate_full(full, failed, self.jobs, **args)
            fast = missing | dict(scope='full', execution_profile='pr-fast')
            ordinary_needs = {key: value for key, value in needs.items()
                              if key not in ('oasis7-test-build', 'full-regression', 'candidate-full-regression')}
            self.assertTrue(result.validate(fast, ordinary_needs, self.jobs))

    def test_full_rejects_every_missing_or_unsuccessful_required_job(self):
        for name in ('select', *self.jobs.values(), 'oasis7-test-build', 'full-regression'):
            for state in ('failure', 'cancelled', 'skipped', 'unknown', None):
                plan, needs, args = self.full_fixture()
                needs[name] = {'result': state}
                with self.subTest(job=name, state=state), self.assertRaises(ValueError):
                    result.validate_full(plan, needs, self.jobs, **args)
            del needs[name]
            with self.subTest(job=name, missing=True), self.assertRaises(ValueError):
                result.validate_full(plan, needs, self.jobs, **args)

    def test_full_cannot_hide_platform_by_omitting_inventory(self):
        for groups in ([], ['baseline'], ['baseline', 'rust', 'rust'], ['baseline', {}]):
            plan, needs, args = self.full_fixture()
            plan['groups'] = groups
            with self.subTest(groups=groups), self.assertRaises(ValueError):
                result.validate_full(plan, needs, self.jobs, **args)

    def test_full_checks_exact_identity_profile_and_inventory_version(self):
        for field, value in (('source_sha', 'c' * 40), ('tested_sha', 'c' * 40),
                             ('policy_sha', 'c' * 40), ('source_sha', 'main'),
                             ('execution_profile', 'pr-fast'), ('check_inventory_version', True),
                             ('check_inventory_version', 2), ('scope', 'full')):
            plan, needs, args = self.full_fixture()
            plan[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                result.validate_full(plan, needs, self.jobs, **args)

    def test_full_candidate_requires_read_only_full_lane_not_main_success(self):
        for state in ('success', 'failure', 'cancelled', None):
            plan, needs, args = self.full_fixture(candidate=True)
            needs['full-regression']['result'] = state
            with self.subTest(state=state), self.assertRaises(ValueError):
                result.validate_full(plan, needs, self.jobs, **args)
        for event, ref, mode in (('push', 'refs/heads/main', ''),
                                 ('schedule', 'refs/heads/candidate', ''),
                                 ('workflow_dispatch', 'refs/heads/main', 'newapi_bridge_package')):
            plan, needs, args = self.full_fixture()
            args.update(event_name=event, ref=ref, run_mode=mode)
            with self.subTest(event=event, ref=ref, mode=mode), self.assertRaises(ValueError):
                result.validate_full(plan, needs, self.jobs, **args)

    def test_consolidated_group_jobs_require_host_success_and_retired_skip(self):
        config = {
            'groups': ['baseline', 'doc_checker_contracts', 'codex_agent_config_validation',
                       'macos_package_contract', 'compile_metrics'],
            'contract_job_consolidation_version': 1,
            'group_jobs': {
                'codex_agent_config_validation': 'baseline',
                'macos_package_contract': 'baseline',
                'compile_metrics': 'doc-checker-contracts',
            },
        }
        legacy_plan = {'groups': ['baseline', 'compile_metrics']}
        self.assertEqual(result.retired_group_jobs(config, legacy_plan), set())
        self.assertEqual(result.configured_group_jobs(config, legacy_plan)['compile_metrics'], 'compile-metrics')
        plan = {'groups': ['baseline', 'compile_metrics'], 'contract_job_consolidation_version': 1}
        jobs = result.configured_group_jobs(config, plan)
        retired = result.retired_group_jobs(config, plan)
        self.assertEqual(jobs['compile_metrics'], 'doc-checker-contracts')
        self.assertEqual(retired, {'codex-agent-config-validation', 'macos-package-contract', 'compile-metrics'})
        needs = {
            'select': {'result': 'success'},
            'baseline': {'result': 'success'},
            'doc-checker-contracts': {'result': 'success'},
            'codex-agent-config-validation': {'result': 'skipped'},
            'macos-package-contract': {'result': 'skipped'},
            'compile-metrics': {'result': 'skipped'},
        }
        self.assertTrue(result.validate(plan, needs, jobs, retired_jobs=retired))
        for bad in ('failure', 'cancelled', 'skipped', None):
            broken = copy.deepcopy(needs)
            broken['doc-checker-contracts'] = {'result': bad}
            with self.subTest(host_state=bad), self.assertRaises(ValueError):
                result.validate(plan, broken, jobs, retired_jobs=retired)
        for retired_job in sorted(retired):
            for bad in ('success', 'failure', 'cancelled', 'unknown', None, 'missing'):
                broken = copy.deepcopy(needs)
                if bad == 'missing':
                    del broken[retired_job]
                else:
                    broken[retired_job] = {'result': bad}
                with self.subTest(retired_job=retired_job, state=bad), self.assertRaises(ValueError):
                    result.validate(plan, broken, jobs, retired_jobs=retired)

    def test_full_accepts_direct_ordinary_producer_xor_and_rejects_ambiguous_lanes(self):
        for candidate, dispatch in ((False, False), (False, True), (True, True)):
            plan, needs, args = self.full_fixture(candidate=candidate, main_dispatch=dispatch)
            del needs['oasis7-test-build']
            needs['oasis7-test-build-candidate'] = {'result': 'success' if candidate else 'skipped'}
            needs['oasis7-test-build-trusted'] = {'result': 'skipped' if candidate else 'success'}
            self.assertTrue(result.validate_full(plan, needs, self.jobs, **args))
            for cand_state, trust_state in (('success', 'success'), ('skipped', 'skipped'),
                                            ('failure', 'skipped'), ('skipped', 'failure'),
                                            ('cancelled', 'skipped'), (None, 'success')):
                broken = copy.deepcopy(needs)
                broken['oasis7-test-build-candidate'] = {'result': cand_state}
                broken['oasis7-test-build-trusted'] = {'result': trust_state}
                with self.subTest(candidate=candidate, cand_state=cand_state, trust_state=trust_state), \
                     self.assertRaises(ValueError):
                    result.validate_full(plan, broken, self.jobs, **args)
            swapped = copy.deepcopy(needs)
            swapped['oasis7-test-build-candidate'] = {'result': 'skipped' if candidate else 'success'}
            swapped['oasis7-test-build-trusted'] = {'result': 'success' if candidate else 'skipped'}
            with self.assertRaises(ValueError):
                result.validate_full(plan, swapped, self.jobs, **args)

    def test_v2_consolidation_v1_transition_and_direct_workspace_support_gate(self):
        config = {
            'groups': [
                'baseline', 'doc_checker_contracts', 'codex_agent_config_validation',
                'macos_package_contract', 'compile_metrics',
                'node', 'consensus', 'scenario_regression', 'pixel_world_bridge',
                'viewer_js_required', 'viewer_performance_report', 'workspace_support',
            ],
            'contract_job_consolidation_version': 2,
            'workspace_support_direct_gate_version': 1,
            'group_jobs': {
                'codex_agent_config_validation': 'baseline',
                'macos_package_contract': 'baseline',
                'compile_metrics': 'doc-checker-contracts',
                'consensus': 'node',
                'pixel_world_bridge': 'scenario-regression',
                'viewer_performance_report': 'viewer-js-required',
            },
        }
        v1_plan = {'groups': ['baseline', 'consensus', 'compile_metrics'],
                   'contract_job_consolidation_version': 1}
        v1_jobs = result.configured_group_jobs(config, v1_plan)
        v1_retired = result.retired_group_jobs(config, v1_plan)
        self.assertEqual(v1_jobs['consensus'], 'consensus')
        self.assertEqual(v1_jobs['compile_metrics'], 'doc-checker-contracts')
        self.assertEqual(v1_retired, {'codex-agent-config-validation', 'macos-package-contract', 'compile-metrics'})

        v2_plan = {
            'groups': ['baseline', 'consensus', 'pixel_world_bridge', 'viewer_performance_report', 'workspace_support'],
            'contract_job_consolidation_version': 2,
            'workspace_support_partition_version': 1,
            'workspace_support_partitions': ['other'],
            'workspace_support_direct_gate_version': 1,
        }
        v2_jobs = result.configured_group_jobs(config, v2_plan)
        v2_retired = result.retired_group_jobs(config, v2_plan)
        self.assertEqual(v2_jobs['consensus'], 'node')
        self.assertEqual(v2_jobs['pixel_world_bridge'], 'scenario-regression')
        self.assertEqual(v2_jobs['viewer_performance_report'], 'viewer-js-required')
        self.assertEqual(v2_jobs['workspace_support'], 'workspace-support-other')
        self.assertIn('workspace-support', v2_retired)
        self.assertTrue({'consensus', 'pixel-world-bridge', 'viewer-performance-report'}.issubset(v2_retired))
        needs = {
            'select': {'result': 'success'},
            'baseline': {'result': 'success'},
            'doc-checker-contracts': {'result': 'skipped'},
            'node': {'result': 'success'},
            'scenario-regression': {'result': 'success'},
            'viewer-js-required': {'result': 'success'},
            'workspace-support-other': {'result': 'success'},
            **{job: {'result': 'skipped'} for job in v2_retired},
        }
        self.assertTrue(result.validate(v2_plan, needs, v2_jobs, retired_jobs=v2_retired))
        for bad in ('skipped', 'failure', 'cancelled', None):
            broken = copy.deepcopy(needs)
            broken['workspace-support-other'] = {'result': bad}
            with self.subTest(direct_support=bad), self.assertRaises(ValueError):
                result.validate(v2_plan, broken, v2_jobs, retired_jobs=v2_retired)
        broken_join = copy.deepcopy(needs)
        broken_join['workspace-support'] = {'result': 'success'}
        with self.assertRaisesRegex(ValueError, 'workspace-support: retired'):
            result.validate(v2_plan, broken_join, v2_jobs, retired_jobs=v2_retired)

        # When workspace_support is unselected or uses custody split, workspace-support-other in needs may be skipped/success but never failed.
        non_support_plan = {'groups': ['baseline'], 'contract_job_consolidation_version': 2}
        ns_jobs = result.configured_group_jobs(config, non_support_plan)
        ns_retired = result.retired_group_jobs(config, non_support_plan)
        ns_needs = {
            'select': {'result': 'success'},
            'baseline': {'result': 'success'},
            'doc-checker-contracts': {'result': 'skipped'},
            'node': {'result': 'skipped'},
            'scenario-regression': {'result': 'skipped'},
            'viewer-js-required': {'result': 'skipped'},
            'workspace-support': {'result': 'skipped'},
            'workspace-support-other': {'result': 'skipped'},
            **{job: {'result': 'skipped'} for job in ns_retired},
        }
        self.assertTrue(result.validate(non_support_plan, ns_needs, ns_jobs, retired_jobs=ns_retired))
        for bad in ('failure', 'cancelled', 'unknown', None):
            broken = copy.deepcopy(ns_needs)
            broken['workspace-support-other'] = {'result': bad}
            with self.subTest(unselected_support_other=bad), self.assertRaises(ValueError):
                result.validate(non_support_plan, broken, ns_jobs, retired_jobs=ns_retired)

    def test_v3_consolidation_and_v1_v2_transitions(self):
        config = {
            'groups': [
                'baseline', 'doc_checker_contracts', 'codex_agent_config_validation',
                'macos_package_contract', 'compile_metrics',
                'node', 'consensus', 'scenario_regression', 'pixel_world_bridge',
                'viewer_js_required', 'viewer_performance_report',
                'packaging_contracts', 'cargo_tooling_contracts',
                'fleet_health', 'windows_rollout',
            ],
            'contract_job_consolidation_version': 3,
            'group_jobs': {
                'codex_agent_config_validation': 'baseline',
                'macos_package_contract': 'baseline',
                'compile_metrics': 'doc-checker-contracts',
                'consensus': 'node',
                'pixel_world_bridge': 'scenario-regression',
                'viewer_performance_report': 'viewer-js-required',
                'packaging_contracts': 'doc-checker-contracts',
                'cargo_tooling_contracts': 'scenario-regression',
                'windows_rollout': 'fleet-health',
            },
        }
        v1_jobs = result.configured_group_jobs(config, {'groups': ['baseline'], 'contract_job_consolidation_version': 1})
        self.assertEqual(v1_jobs['consensus'], 'consensus')
        self.assertEqual(v1_jobs['packaging_contracts'], 'packaging-contracts')
        self.assertEqual(v1_jobs['windows_rollout'], 'windows-rollout')

        v2_jobs = result.configured_group_jobs(config, {'groups': ['baseline'], 'contract_job_consolidation_version': 2})
        self.assertEqual(v2_jobs['consensus'], 'node')
        self.assertEqual(v2_jobs['packaging_contracts'], 'packaging-contracts')
        self.assertEqual(v2_jobs['cargo_tooling_contracts'], 'cargo-tooling-contracts')
        self.assertEqual(v2_jobs['windows_rollout'], 'windows-rollout')

        v3_plan = {'groups': ['baseline', 'packaging_contracts', 'cargo_tooling_contracts', 'windows_rollout'],
                   'contract_job_consolidation_version': 3}
        v3_jobs = result.configured_group_jobs(config, v3_plan)
        v3_retired = result.retired_group_jobs(config, v3_plan)
        self.assertEqual(v3_jobs['packaging_contracts'], 'doc-checker-contracts')
        self.assertEqual(v3_jobs['cargo_tooling_contracts'], 'scenario-regression')
        self.assertEqual(v3_jobs['windows_rollout'], 'fleet-health')
        self.assertTrue({'packaging-contracts', 'cargo-tooling-contracts', 'windows-rollout'}.issubset(v3_retired))
        needs = {
            'select': {'result': 'success'},
            'baseline': {'result': 'success'},
            'doc-checker-contracts': {'result': 'success'},
            'fleet-health': {'result': 'success'},
            'node': {'result': 'skipped'},
            'scenario-regression': {'result': 'success'},
            'viewer-js-required': {'result': 'skipped'},
            **{job: {'result': 'skipped'} for job in v3_retired},
        }
        self.assertTrue(result.validate(v3_plan, needs, v3_jobs, retired_jobs=v3_retired))
        for retired_job in ('packaging-contracts', 'cargo-tooling-contracts', 'windows-rollout'):
            broken = copy.deepcopy(needs)
            broken[retired_job] = {'result': 'success'}
            with self.subTest(retired_job=retired_job), self.assertRaises(ValueError):
                result.validate(v3_plan, broken, v3_jobs, retired_jobs=v3_retired)


if __name__ == '__main__':
    unittest.main()
