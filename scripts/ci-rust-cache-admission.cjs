// Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
// NODE_OPTIONS loads this before the pinned action. Missing configuration or
// an unknown action entry must stop this optional action, never disable its guard.
'use strict';
require('./ci-rust-cache-admission-hook.cjs').installGuard();
