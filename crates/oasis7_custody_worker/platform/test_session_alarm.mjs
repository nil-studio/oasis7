// Private DO scheduling glue only. Rust owns all deadlines and expiry policy.
// Queue only these storage operations, never the Account's fetch/workflow.
const queues = new WeakMap();
function enqueue(state, operation) {
  if (!state || typeof state !== 'object' || !state.storage)
    throw new TypeError('invalid alarm state');
  const previous = queues.get(state) ?? Promise.resolve();
  const running = previous.then(operation);
  // A failed caller still receives its rejection. Only the queue tail recovers
  // so later repair is not permanently blocked by an earlier storage failure.
  queues.set(state, running.then(() => undefined, () => undefined));
  return running;
}
function deadline(raw, optional = false) {
  if (optional && raw === '') return null;
  if (typeof raw !== 'string' || !/^[1-9][0-9]*$/.test(raw))
    throw new TypeError('invalid alarm deadline');
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0 || String(value) !== raw)
    throw new TypeError('invalid alarm deadline');
  return value;
}
function actualAlarm(value) {
  if (value === null) return null;
  if (!Number.isSafeInteger(value) || value < 0)
    throw new TypeError('invalid stored alarm');
  return value;
}
export function prearmTestSessionAlarm(state, raw) {
  const candidate = deadline(raw);
  return enqueue(state, async () => {
    const existing = actualAlarm(await state.storage.getAlarm());
    if (existing === null || candidate < existing)
      await state.storage.setAlarm(candidate);
  });
}
export function prearmAndCommitTestSessionAlarm(state, raw, commit) {
  const candidate = deadline(raw);
  if (typeof commit !== 'function') throw new TypeError('invalid alarm commit');
  return enqueue(state, async () => {
    const existing = actualAlarm(await state.storage.getAlarm());
    if (existing === null || candidate < existing)
      await state.storage.setAlarm(candidate);
    // There is deliberately no await between arming and this synchronous SQL
    // callback. The scheduling queue still owns this task, so a reconcile
    // cannot observe the pre-commit snapshot and delete its pending alarm.
    const result = commit();
    if (result && typeof result.then === 'function')
      throw new TypeError('alarm commit must be synchronous');
    return result;
  });
}
// The synchronous Rust callback MUST reread the actual SQL state and apply local
// expiry. It runs after the storage await, not before it on a stale snapshot.
// It returns '' only after successfully proving no remaining live deadline.
export function reconcileTestSessionAlarm(state, calculate) {
  if (typeof calculate !== 'function') throw new TypeError('invalid alarm callback');
  return enqueue(state, async () => {
    const existing = actualAlarm(await state.storage.getAlarm());
    const raw = calculate();
    const candidate = deadline(raw, true);
    if (candidate === null) {
      await state.storage.deleteAlarm();
    } else if (existing === null || candidate < existing || existing <= Date.now()) {
      await state.storage.setAlarm(candidate);
    }
    // A retained earlier alarm is intentional. Its handler must recompute from
    // SQL, and must not close a lease merely because the old timer fired early.
    return raw;
  });
}

export function testSessionAlarmPresent(state) {
  const sql = state.storage.sql;
  if (!sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name='custody_records'").toArray().length) return false;
  return sql.exec("SELECT id FROM custody_records WHERE id = ?", 'custody-provision-approval-v1').toArray().length === 1;
}
export function retryTestSessionAlarm(state) {
  return enqueue(state, async () => {
    const now = Date.now();
    const existing = actualAlarm(await state.storage.getAlarm());
    const retry = now + 60000;
    if (existing === null || existing <= now || retry < existing)
      await state.storage.setAlarm(retry);
  });
}
