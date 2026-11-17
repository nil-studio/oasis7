#!/usr/bin/env python3
"""Explicit hosted GDB process contracts; never a skipped governance witness."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from ci_process_runner import run

SPEC = importlib.util.spec_from_file_location('capture', Path(__file__).with_name('ci-custody-workerd-capture.py'))
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)
ROOT = Path(__file__).resolve().parents[1]
CONTEXT = dict(source_sha='a' * 40, tested_sha='b' * 40, policy_sha='c' * 40, run_id='71')

C_FIXTURE = r'''
/* Ordinary child-process contracts for standard GDB; no memory fault injection. */
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile sig_atomic_t recovered;

static void recover(int signum) {
  (void)signum;
  recovered = 1;
}

static void fail(int signum) {
  (void)signum;
  _exit(23);
}

int main(int argc, char **argv) {
  if (argc < 2) return 90;
  if (strcmp(argv[1], "recover") == 0 || strcmp(argv[1], "fatal") == 0) {
    struct sigaction action = {0};
    action.sa_handler = strcmp(argv[1], "recover") == 0 ? recover : fail;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGSEGV, &action, NULL) != 0 || raise(SIGSEGV) != 0) return 91;
    return recovered ? 0 : 92;
  }
  if (strcmp(argv[1], "wait") == 0) {
    if (write(3, "READY", 5) != 5) return 93;
    for (;;) pause();
  }
  if (strcmp(argv[1], "exit") == 0 && argc == 3) return atoi(argv[2]);
  if (strcmp(argv[1], "io") == 0 && argc == 3) {
    const char expected[] = "config\0payload";
    const char reply[] = "control\0reply\n";
    const char argument[] = "a space 'quote' \"double\" $literal;with\\backslash";
    char input[sizeof(expected)] = {0};
    size_t count = 0;
    ssize_t received;
    while ((received = read(0, input + count, sizeof(input) - count)) > 0) {
      count += (size_t)received;
      if (count == sizeof(input)) return 94;
    }
    if (received < 0 || count != sizeof(expected) - 1 || memcmp(input, expected, count)
        || strcmp(argv[2], argument)) return 95;
    if (write(3, reply, sizeof(reply) - 1) != (ssize_t)(sizeof(reply) - 1)) return 96;
    puts("fixture stdout");
    fputs("fixture stderr\n", stderr);
    return 0;
  }
  return 97;
}
'''

NODE_FIXTURE = r'''
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { test } from 'node:test';

function start(mode, ...args) {
  const child = spawn(process.env.WORKERD_DEBUG_FIXTURE, [mode, ...args], {
    stdio: ['pipe', 'pipe', 'pipe', 'pipe'],
  });
  const stdout = [], stderr = [], control = [];
  child.stdout.on('data', data => stdout.push(data));
  child.stderr.on('data', data => stderr.push(data));
  child.stdio[3].on('data', data => control.push(data));
  const closed = once(child, 'close');
  child.stdin.end(mode === 'io' ? Buffer.from('config\0payload') : undefined);
  return { child, closed, stdout, stderr, control };
}

async function io() {
  const running = start('io', 'a space \'quote\' "double" $literal;with\\backslash');
  assert.deepEqual(await running.closed, [0, null]);
  assert.equal(Buffer.concat(running.stdout).toString(), 'fixture stdout\n');
  assert.equal(Buffer.concat(running.stderr).toString(), 'fixture stderr\n');
  assert.deepEqual(Buffer.concat(running.control), Buffer.from('control\0reply\n'));
}

async function killed() {
  const running = start('wait');
  await once(running.child.stdio[3], 'data');
  assert.equal(Buffer.concat(running.control).toString(), 'READY');
  assert.equal(running.child.kill('SIGKILL'), true);
  assert.deepEqual(await running.closed, [null, 'SIGKILL']);
}

test('standard debugger preserves original child contracts', { timeout: 15_000 }, async () => {
  switch (process.env.WORKERD_DEBUG_MODE) {
    case 'io': await io(); break;
    case 'recover': assert.deepEqual(await start('recover').closed, [0, null]); break;
    case 'fatal': assert.deepEqual(await start('fatal').closed, [23, null]); break;
    case 'restart':
      assert.deepEqual(await start('fatal').closed, [23, null]);
      await io();
      break;
    case 'dispose': await killed(); break;
    case 'mixed':
      await Promise.all([killed(), (async () => {
        assert.deepEqual(await start('exit', '23').closed, [23, null]);
      })()]);
      break;
    case 'root-failure':
      await io();
      assert.fail('intentional original Node failure');
      break;
    default: assert.fail('unknown debugger contract mode');
  }
});
'''


class GdbContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if os.environ.get('OASIS7_REQUIRE_GDB_TESTS') != '1':
            raise AssertionError('real GDB contracts require explicit host enablement')
        cls.gdb = shutil.which('gdb')
        cls.node = shutil.which('node')
        if cls.gdb is None or cls.node is None or shutil.which('cc') is None:
            raise AssertionError('enabled GDB contracts require gdb, Node and cc')
        if os.environ.get('OASIS7_GDB_CONTRACT_RESULTS'):
            cls.directory = Path(os.environ['OASIS7_GDB_CONTRACT_RESULTS']).resolve()
            cls.directory.mkdir(parents=True, exist_ok=False)
        else:
            cls.temporary = tempfile.TemporaryDirectory(prefix='workerd gdb contracts ')
            cls.addClassCleanup(cls.temporary.cleanup)
            cls.directory = Path(cls.temporary.name)
        cls.binary = cls.directory / 'ordinary-child'
        source = cls.directory / 'ordinary-child.c'
        source.write_text(C_FIXTURE)
        cls.node_fixture = cls.directory / 'contract.test.mjs'
        cls.node_fixture.write_text(NODE_FIXTURE)
        subprocess.run(['cc', '-std=c11', '-D_POSIX_C_SOURCE=200809L', '-Wall', '-Wextra', '-Werror',
                        '-O0', '-g', str(source),
                        '-o', str(cls.binary)], check=True, timeout=30)
        cls.engine_sha = hashlib.sha256(cls.binary.read_bytes()).hexdigest()

    def observe(self, mode):
        directory = self.directory / mode
        directory.mkdir()
        config = dict(schema=C.SCHEMA, context=CONTEXT, attempt=2, engine_sha256=self.engine_sha,
                      executable=str(self.binary), gdb=self.gdb,
                      driver=str(ROOT / 'scripts/ci-custody-workerd-gdb.py'),
                      records=str(directory / 'records.jsonl'))
        config_path = directory / 'config.json'
        config_path.write_text(json.dumps(config))
        command = C.debug_command([self.node, '--test', '--test-concurrency=1', '--test-reporter=tap',
                                   str(self.node_fixture)], config)
        environment = dict(os.environ, OASIS7_WORKERD_GDB_CONFIG=str(config_path), DEBUGINFOD_URLS='',
                           WORKERD_DEBUG_FIXTURE=str(self.binary), WORKERD_DEBUG_MODE=mode)
        with (directory / 'node.log').open('w') as output:
            result = run(command, ROOT, environment, 'workerd-gdb-contract', mode,
                         directory / 'processes.jsonl', timeout_seconds=45, log_stream=output)
        log = (directory / 'node.log').read_text()
        self.assertEqual(result['exit_code'], 1 if mode == 'root-failure' else 0, log)
        self.assertFalse(result['timeout'], log)
        self.assertFalse(result['leaked_processes'], log)
        self.assertEqual(result['cleanup_result'], 'clean', log)
        self.assertTrue(result['log_complete'], log)
        data = C.read_regular(directory / 'records.jsonl')
        with mock.patch.object(C, 'ENGINE_SHA256', self.engine_sha):
            summary = C.summarize(data, CONTEXT, 2)
        return summary, [json.loads(line) for line in data.splitlines()]

    def test_binary_stdin_fd3_arguments_and_streams(self):
        summary, unused = self.observe('io')
        self.assertTrue(summary['passed'])

    def test_recoverable_signal_reaches_original_handler(self):
        summary, rows = self.observe('recover')
        self.assertTrue(summary['passed'])
        signals = [row for row in rows if row['event'] == 'signal']
        self.assertEqual(len(signals), 1)
        self.assertEqual(signals[0]['signal'], 'SIGSEGV')
        self.assertLessEqual(signals[0]['si_code'], 0)
        self.assertIsNone(signals[0]['si_addr'])

    def test_original_fatal_handler_keeps_exit_23(self):
        summary, unused = self.observe('fatal')
        self.assertFalse(summary['passed'])
        self.assertEqual(summary['terminations'][0]['exit_code'], 23)

    def test_restart_success_cannot_clear_first_failure(self):
        summary, unused = self.observe('restart')
        self.assertEqual(summary['root_exit_code'], 0)
        self.assertEqual(summary['runtime_count'], 2)
        self.assertFalse(summary['passed'])

    def test_sigkill_disposal_finishes_without_a_wrapper_or_orphan(self):
        summary, unused = self.observe('dispose')
        self.assertTrue(summary['passed'])
        self.assertEqual(summary['terminations'][0]['exit_signal'], 9)

    def test_distinct_child_exit_statuses_do_not_cross_inferiors(self):
        summary, unused = self.observe('mixed')
        self.assertFalse(summary['passed'])
        self.assertCountEqual([(row['exit_code'], row['exit_signal']) for row in summary['terminations']],
                              [(23, None), (None, 9)])

    def test_root_node_failure_remains_the_process_exit_status(self):
        summary, unused = self.observe('root-failure')
        self.assertEqual(summary['root_exit_code'], 1)
        self.assertFalse(summary['passed'])


if __name__ == '__main__':
    unittest.main()
