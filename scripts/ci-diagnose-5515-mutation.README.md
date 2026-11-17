# Fixed full-sync mutation diagnostic

This optional, manual diagnostic tests whether the real signed-HTTP failover
regression detects loss of cumulative material charges. It is not a required
gate, production qualification, or a replacement for the original test suite.

The workflow runs only from `main`, with read-only repository permission and
no dispatch inputs. The runner is taken from the workflow commit. The tested
source is fixed to `ef4f2cee67b696430d28ce6062c45eae2638364e`, which includes
the merged #5918 regression. Updating that source, mutation, or target requires
a reviewed code change; arbitrary candidate commands and patches are not accepted.

Only the isolated `experiment-source` checkout is modified. The runner verifies
its commit, clean tracked tree and mutation-file SHA-256, then runs the exact
single Rust test on the original source, a budget-reset mutation, and the restored
source. The RED oracle requires that specific test and its budget assertion to
fail; compilation errors, zero tests, unrelated panics and timeouts do not qualify.
The original file is restored in `finally`, and tracked cleanliness is checked.
A normal SIGTERM becomes a cancellation exception: the owned child group is
killed and reaped, an incomplete phase is recorded, and restoration completes
with further SIGTERM ignored during cleanup. The prior handler is then restored.
A hard runner/VM termination can prevent that finalizer; the disposable hosted
checkout is never pushed or reused as a developer workspace.

Cost is one Ubuntu runner, at most 90 minutes including setup; the three cargo
phases share a 65-minute absolute deadline and a checkout-local target. No cache
is saved. Output is capped at 8 MiB per phase; only JSON phase metadata and output
hashes are uploaded, not logs, world stores or private fixture material. Timeout
and output-limit failure terminate and reap the owned process group.

The workflow must first be reviewed and merged by an authorized maintainer before
it can run from main. A draft PR, Python mechanics tests or a normal Rust gate
does not establish mutation RED/GREEN. Check all three accepted phase receipts,
restoration and the workflow outcome. A successful result covers this one fixed
mutation, not all possible failover defects or the remainder of #5515.
