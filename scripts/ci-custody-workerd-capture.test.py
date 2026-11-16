#!/usr/bin/env python3
"""Pure capture receipts and failure propagation, without a debugger."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location('capture', Path(__file__).with_name('ci-custody-workerd-capture.py'))
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)

CONTEXT = dict(source_sha='a' * 40, tested_sha='b' * 40, policy_sha='c' * 40, run_id='71')


def records(code=0, sig=None):
    return [dict(event='session', schema=C.SCHEMA, context=CONTEXT, attempt=2,
                 engine_sha256=C.ENGINE_SHA256, gdb_version='fixture-only'),
            dict(event='root_started', inferior=1, pid=101),
            dict(event='runtime_started', inferior=3, pid=103, engine_sha256=C.ENGINE_SHA256),
            dict(event='runtime_exited', inferior=3, pid=103, exit_code=code, exit_signal=sig),
            dict(event='root_exited', inferior=1, pid=101, exit_code=0),
            dict(event='complete', errors=[], open_runtimes=[])]


def encoded(rows):
    return ('\n'.join(json.dumps(row) for row in rows) + '\n').encode()


def signal_record(offset='0x1234'):
    return dict(event='signal', inferior=3, pid=103, thread=[103, 103, 0], signal='SIGSEGV', si_code=1,
                si_addr='0x0', registers=dict(pc='0x10001234', r13='0x0', rbp='0x1234', rsp='0x5678'),
                maps_complete=True, mappings=dict.fromkeys(C.MAPPINGS), pc_offset=offset)


class Receipts(unittest.TestCase):
    def test_single_runtime_cleanup_is_known_signal_with_unknown_origin(self):
        summary = C.summarize(encoded(records(None, 9)), CONTEXT, 2)
        self.assertTrue(summary['passed'])
        self.assertEqual(summary['terminations'], [dict(exit_code=None, exit_signal=9,
            origin='unknown', compatible_with_dispose=True)])

    def test_recovered_signal_is_not_automatically_fatal(self):
        rows = records()
        rows.insert(3, signal_record())
        self.assertTrue(C.summarize(encoded(rows), CONTEXT, 2)['passed'])

    def test_known_fault_and_nonzero_exit_remain_failed_after_root_zero(self):
        rows = records()
        rows.insert(3, signal_record(C.KNOWN_FAULT_OFFSET))
        self.assertFalse(C.summarize(encoded(rows), CONTEXT, 2)['passed'])
        summary = C.summarize(encoded(records(23)), CONTEXT, 2)
        self.assertTrue(summary['unexpected_exit'])
        self.assertFalse(summary['passed'])

    def test_second_runtime_cannot_turn_a_first_failure_into_success(self):
        for first_code, first_signal in ((23, None), (None, 9), (0, None)):
            rows = records(first_code, first_signal)
            rows[4:4] = [dict(event='runtime_started', inferior=4, pid=104, engine_sha256=C.ENGINE_SHA256),
                         dict(event='runtime_exited', inferior=4, pid=104, exit_code=0, exit_signal=None)]
            summary = C.summarize(encoded(rows), CONTEXT, 2)
            self.assertEqual(summary['runtime_count'], 2)
            self.assertFalse(summary['passed'])

    def test_unknown_exit_and_incomplete_observation_are_rejected(self):
        bad = []
        rows = records(None, None)
        bad.append(rows)
        for field, value in [('errors', ['debugger_command_failed']), ('open_runtimes', [[3, 103]])]:
            rows = records()
            rows[-1][field] = value
            bad.append(rows)
        for index in (1, 2, 3, 4, 5):
            rows = records()
            del rows[index]
            bad.append(rows)
        for rows in bad:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                C.summarize(encoded(rows), CONTEXT, 2)

    def test_run_attempt_engine_and_event_process_identity_are_bound(self):
        for change in ({'context': dict(CONTEXT, run_id='72')}, {'attempt': 1},
                       {'attempt': True}, {'engine_sha256': 'd' * 64}):
            rows = records()
            rows[0].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                C.summarize(encoded(rows), CONTEXT, 2)
        for index, field, value in ((3, 'pid', 999), (4, 'inferior', 3), (3, 'exit_code', False)):
            rows = records()
            rows[index][field] = value
            with self.subTest(index=index, field=field), self.assertRaises(ValueError):
                C.summarize(encoded(rows), CONTEXT, 2)

    def test_truncation_unknown_events_and_bounds_do_not_count_as_zero_faults(self):
        for raw in (b'', b'null\n', b'[]\n', encoded(records())[:-1], b'{}\n' * (C.MAX_RECORDS + 1),
                    b' ' * (C.MAX_RECORD_BYTES + 1)):
            with self.subTest(size=len(raw)), self.assertRaises(ValueError):
                C.summarize(raw, CONTEXT, 2)
        rows = records()
        rows.insert(3, dict(event='unexpected'))
        with self.assertRaises(ValueError):
            C.summarize(encoded(rows), CONTEXT, 2)

    def test_missing_registers_or_partial_maps_are_not_complete_evidence(self):
        for field, value in (('maps_complete', False), ('registers', {'pc': '0x1234'}),
                             ('si_addr', 'arbitrary data'), ('mappings', {})):
            rows = records()
            observation = signal_record()
            observation[field] = value
            rows.insert(3, observation)
            with self.subTest(field=field), self.assertRaises(ValueError):
                C.summarize(encoded(rows), CONTEXT, 2)

    def test_final_consumer_recomputes_records_and_never_trusts_report_alone(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            path = directory / 'records.jsonl'
            path.write_bytes(encoded(records()))
            self.assertTrue(C.finish(directory, CONTEXT, 2, True)['passed'])
            C.validate(directory, CONTEXT, 2)
            path.write_bytes(encoded(records(23)))
            with self.assertRaises(ValueError):
                C.validate(directory, CONTEXT, 2)
            self.assertFalse(C.finish(directory, CONTEXT, 2, False)['passed'])
            path.unlink()
            self.assertFalse(C.finish(directory, CONTEXT, 2, True)['complete'])

    def test_symlink_and_nonregular_records_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            outside = directory / 'other'
            outside.write_bytes(encoded(records()))
            path = directory / 'records.jsonl'
            path.symlink_to(outside)
            self.assertFalse(C.finish(directory, CONTEXT, 2, True)['passed'])
            path.unlink()
            os.mkfifo(path)
            self.assertFalse(C.finish(directory, CONTEXT, 2, True)['passed'])

    def test_debugger_command_preserves_argument_boundaries_and_explicit_startup(self):
        original = ['/node with space', '--test', 'a;literal.test.mjs']
        config = dict(gdb='/usr/bin/gdb', driver='/a directory/driver.py')
        command = C.debug_command(original, config)
        self.assertEqual(command[command.index('--args') + 1:], original)
        self.assertIn('--nx', command)
        self.assertIn('--nh', command)
        self.assertIn('--command=/a directory/driver.py', command)

    def test_wrong_engine_and_runtime_override_fail_before_any_debugger_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'crates/oasis7_custody_worker/node_modules/@cloudflare/workerd-linux-64/bin/workerd'
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b'wrong engine')
            for environment in ({'MINIFLARE_WORKERD_PATH': '/override'}, {}, {'LD_PRELOAD': '/override'}):
                with self.subTest(environment=environment), self.assertRaises(ValueError):
                    C.prepare(root, root, environment, CONTEXT, 2, ['node'])




if __name__ == '__main__':
    unittest.main()
