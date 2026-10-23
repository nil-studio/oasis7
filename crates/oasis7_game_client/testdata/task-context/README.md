# Canonical task authority fixtures

These synthetic snapshots and paired signed requests were exported from the
Chain private task context provider/native regression scenarios. They contain
fixed test keys and synthetic goals, not real account or player data. The request
executor uses seed `[73; 32]`; reply tests use a separate service seed `[9; 32]`.

Regenerate in a new absolute mode-0700 directory with
`OASIS7_TASK_TEST_FIXTURE_DIR=<directory>` while running the Chain
`private_task_context_` test filter, one thread. The test-only exporter uses
create-new mode-0600 files. Copy these four outputs only after the producer tests
pass; SDK tests independently validate both snapshots and the original signatures
before testing mismatched modes. No fixture export environment is needed to run
SDK tests.
