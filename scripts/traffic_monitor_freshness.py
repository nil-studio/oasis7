"""Collector freshness is independent of producer observation and historical deltas."""
import time


def timestamp(value):
    return value if type(value) is int and value >= 0 else None


def capture(record):
    return timestamp(record.get('captured_at_unix_ms'))


def identity(record):
    traffic = record.get('traffic') or {}
    return (record.get('node_id'), record.get('world_id'),
            *((traffic.get(lane) or {}).get('observed_since_unix_ms')
              for lane in ('udp_gossip', 'libp2p_replication')))


def counter_reset(old, new):
    # Only compare cumulative fields consumed by the traffic reducer. Producer
    # snapshots also contain gauges (e.g. discovery_work.inflight/window_starts)
    # and policy metadata; their normal decrease is not a counter incarnation.
    def counters(record):
        found = {}

        def direction(entry, prefix, fields):
            if not isinstance(entry, dict):
                return
            for side in ('inbound', 'outbound'):
                values = entry.get(side)
                if not isinstance(values, dict):
                    continue
                for field in fields:
                    value = values.get(field)
                    if type(value) is int:
                        found[prefix + (side, field)] = value

        traffic = record.get('traffic') or {}
        for lane_name in ('udp_gossip', 'libp2p_replication'):
            lane = traffic.get(lane_name) or {}
            count = 'datagrams' if lane_name == 'udp_gossip' else 'messages'
            fields = (count, 'payload_bytes')
            direction(lane, (lane_name,), fields)
            for group in ('totals', 'gossip', 'request', 'response'):
                direction(lane.get(group), (lane_name, group), fields)
            direction(lane.get('wire_totals'), (lane_name, 'wire_totals'), ('bytes',))
            for group in ('by_kind', 'by_topic', 'by_protocol'):
                for name, entry in (lane.get(group) or {}).items():
                    direction(entry, (lane_name, group, name), fields)
            control = lane.get('control_plane') or {}
            value = control.get('total_events')
            if type(value) is int:
                found[(lane_name, 'control_plane', 'total_events')] = value
            for name, value in (control.get('by_kind') or {}).items():
                if type(value) is int:
                    found[(lane_name, 'control_plane', 'by_kind', name)] = value
            direction(control.get('wire_bytes'), (lane_name, 'control_plane', 'wire_bytes'), ('bytes',))
        for field in ('rx_bytes', 'tx_bytes'):
            value = (record.get('network_interface') or {}).get(field)
            if type(value) is int:
                found[('network_interface', field)] = value
        return found

    previous, current = counters(old), counters(new)
    return any(current[key] < previous[key] for key in previous.keys() & current.keys())


def analyze(records, as_of, window_minutes, max_age, max_gap, historical=False):
    start = as_of - window_minutes * 60_000
    visible = [r for r in records if capture(r) is None or capture(r) <= as_of]
    timed = sorted((r for r in visible if capture(r) is not None), key=capture)
    latest = visible[-1] if visible else None
    successes = [r for r in timed if r.get('status_fetch_ok') is True]
    success = successes[-1] if successes else None
    previous = None
    regressed = False
    producer_previous = None
    producer_regressed = False
    for row in visible:
        ts = capture(row)
        if ts is not None:
            regressed |= previous is not None and ts < previous
            previous = ts
        observed = timestamp(row.get('status_observed_at_unix_ms'))
        if row.get('status_fetch_ok') is True and observed is not None:
            producer_regressed |= producer_previous is not None and observed < producer_previous
            producer_previous = observed
    unknown = any(capture(r) is None for r in visible)
    future = not historical and any(capture(r) is not None and capture(r) > as_of for r in records)
    reasons = []
    if regressed:
        reasons.append('collector_clock_regression')
    if unknown or latest is None:
        reasons.append('missing_capture_time')
    if future:
        reasons.append('future_capture_time')
    result = ('success' if latest.get('status_fetch_ok') is True else
              'failed' if latest.get('status_fetch_ok') is False else 'unknown') if latest and capture(latest) is not None else 'unknown'
    age = as_of - capture(latest) if latest and capture(latest) is not None else None
    success_age = as_of - capture(success) if success else None
    state = 'fresh'
    if unknown or regressed or future or result == 'unknown':
        state = 'unknown'
    elif result == 'failed':
        state = 'failed'
        reasons.append('latest_attempt_failed')
    elif age > max_age:
        state = 'stale'
        reasons.append('sample_stale')
    observed = timestamp((success or {}).get('status_observed_at_unix_ms'))
    producer_age = as_of - observed if observed is not None and observed <= as_of else None
    producer_state = 'fresh'
    if observed is None:
        producer_state = 'unknown'
        reasons.append('missing_producer_time')
    elif observed > as_of or (success is not None and observed > capture(success)):
        producer_state = 'unknown'
        reasons.append('future_producer_time')
    elif producer_regressed:
        producer_state = 'unknown'
        reasons.append('producer_clock_regression')
    elif producer_age > max_age:
        producer_state = 'stale'
        reasons.append('producer_stale')
    segment = []
    changed = False
    reset = False
    if success:
        for row in reversed(successes):
            if identity(row) != identity(success):
                changed = True
                break
            if segment and counter_reset(row, segment[-1]):
                reset = True
                break
            segment.append(row)
        segment.reverse()
    in_window = [r for r in timed if capture(r) >= start]
    # A predecessor can establish the left boundary only within the same gap budget.
    predecessor = next((r for r in reversed(timed) if capture(r) < start), None)
    use_predecessor = (predecessor is not None and bool(in_window)
                       and capture(in_window[0]) > start
                       and start - capture(predecessor) <= max_gap)
    coverage_rows = ([predecessor] if use_predecessor else []) + in_window
    successful_window = [r for r in segment if capture(r) >= start]
    successful_predecessor = next((r for r in reversed(segment) if capture(r) < start), None)
    if (successful_predecessor is not None and successful_window
            and capture(successful_window[0]) > start
            and start - capture(successful_predecessor) <= max_gap):
        successful_window.insert(0, successful_predecessor)
    gaps = [capture(b) - capture(a) for a, b in zip(successful_window, successful_window[1:])]
    if successful_window:
        gaps += [max(0, capture(successful_window[0]) - start), as_of - capture(successful_window[-1])]
    else:
        gaps = [as_of - start]
    max_actual_gap = max(gaps)
    failures = sum(r.get('status_fetch_ok') is False for r in in_window)
    known_failures = [r for r in coverage_rows if r.get('status_fetch_ok') is not True]
    window_identity_changed = bool(success) and any(
        r.get('status_fetch_ok') is True and identity(r) != identity(success) for r in coverage_rows)
    window_reset = any(counter_reset(a, b) for a, b in zip(coverage_rows, coverage_rows[1:])
                       if a.get('status_fetch_ok') is True and b.get('status_fetch_ok') is True)
    full = (state == 'fresh' and producer_state == 'fresh' and len(coverage_rows) >= 2 and not known_failures
            and not window_identity_changed and not window_reset
            and capture(coverage_rows[0]) <= start + max_gap
            and max_actual_gap is not None and max_actual_gap <= max_gap
            and as_of - capture(coverage_rows[-1]) <= max_age)
    if failures:
        reasons.append('sample_failed')
    if max_actual_gap is not None and max_actual_gap > max_gap:
        reasons.append('sample_gap')
    if window_identity_changed:
        reasons.append('incarnation_changed')
    if window_reset:
        reasons.append('counter_reset')
    if not full:
        reasons.append('window_incomplete')
    return {
        'as_of_unix_ms': as_of,
        'available': state == 'fresh',
        'historical_data_available': bool(success),
        'latest_attempt': {'captured_at_unix_ms': capture(latest) if latest else None,
                           'result': result, 'error': (latest or {}).get('fetch_error')},
        'last_success': ({'captured_at_unix_ms': capture(success), 'node_id': success.get('node_id'),
                          'world_id': success.get('world_id'),
                          'status_observed_at_unix_ms': observed} if success else None),
        'freshness': {'state': state, 'sample_age_ms': age, 'last_success_age_ms': success_age,
                      'producer': {'state': producer_state, 'observed_at_unix_ms': observed,
                                   'age_ms': producer_age}, 'reasons': reasons},
        'window': {'start_unix_ms': start, 'end_unix_ms': as_of,
                   'failed_sample_count': failures, 'max_sample_gap_ms': max_actual_gap,
                   'full_window_covered': full,
                   'restart_or_counter_reset_detected_within_window': window_identity_changed or window_reset},
    }, segment, visible


def now_ms():
    return time.time_ns() // 1_000_000


def normalize_status_file(path):
    """Turn malformed HTTP-200 bodies into a recorded failed collection attempt."""
    import json
    from pathlib import Path
    object_keys = {'traffic', 'udp_gossip', 'libp2p_replication', 'consensus', 'storage',
                   'reward_runtime', 'totals', 'inbound', 'outbound', 'gossip', 'request',
                   'response', 'wire_totals', 'control_plane', 'wire_bytes'}
    numeric_keys = {'messages', 'datagrams', 'payload_bytes', 'bytes', 'total_events',
                    'committed_height', 'network_committed_height', 'known_peer_heads',
                    'recent_errors_count', 'observed_since_unix_ms'}

    def validate(value, parent=None):
        if not isinstance(value, dict):
            raise ValueError('expected status object')
        for key, item in value.items():
            if item is None:
                continue
            if key in object_keys:
                validate(item, key)
            elif key in ('by_topic', 'by_protocol', 'by_kind'):
                if not isinstance(item, dict):
                    raise ValueError('expected detail map')
                for entry in item.values():
                    if key == 'by_kind' and parent == 'control_plane':
                        if type(entry) is not int or entry < 0:
                            raise ValueError('expected event counter')
                    else:
                        validate(entry)
            elif key in numeric_keys and (type(item) is not int or item < 0):
                raise ValueError('expected counter')
    path = Path(path)
    try:
        value = json.loads(path.read_text(encoding='utf-8'))
        if not isinstance(value, dict) or type(value.get('ok')) is not bool:
            raise ValueError('missing collection result')
        if value['ok']:
            if not all(isinstance(value.get(key), str) and value[key] for key in ('node_id', 'world_id')):
                raise ValueError('missing identity')
            validate(value)
        else:
            error = value.get('fetch_error')
            path.write_text(json.dumps({'ok': False, 'fetch_error': error if isinstance(error, str) else 'status_not_ok'}) + '\n', encoding='utf-8')
    except (ValueError, TypeError, UnicodeError):
        path.write_text('{"ok":false,"fetch_error":"invalid_status_response"}\n', encoding='utf-8')


if __name__ == '__main__':
    import sys
    if sys.argv[1] == '--validate-as-of':
        candidate = int(sys.argv[2])
        if candidate < 0 or candidate > now_ms():
            print('as_of_unix_ms must be nonnegative and cannot exceed current UTC time', file=sys.stderr)
            sys.exit(2)
    else:
        normalize_status_file(sys.argv[1])
        if len(sys.argv) > 2 and sys.argv[2] == "--capture-time":
            print(now_ms())
