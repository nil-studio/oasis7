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


if __name__ == '__main__':
    unittest.main()
