"""Pure, provenance-preserving projection of observations into serial clips.

This module never changes stored observations or counters. Session boundaries can
support inferred continuity; ``restored_ranges`` identifies that inference.
"""
from datetime import datetime, timezone
import hashlib
import json
from functools import lru_cache
from bisect import bisect_left, bisect_right


_SEARCH_STAGES = {'province', 'procedure', 'auth', 'auth_wait', 'applicant',
    'request_menu', 'request_no_slots', 'request_no_slots_restart',
    'post_contact_no_slots'}
_NO_SLOTS = {'request_no_slots', 'request_no_slots_restart', 'post_contact_no_slots'}
_MANUAL = {'manual', 'paused', 'pause', 'booking_handoff'}
_GAPS = {'unobserved', 'sleep', 'suspended'}
_TERMINAL = {'finished', 'stopped'}
_CONFIRMED_REASONS = {'completed', 'stopped', 'failed', 'interrupted', 'historical_finish'}


@lru_cache(maxsize=32768)
def _time(value):
    if isinstance(value, datetime):
        result = value
    else:
        result = datetime.fromisoformat(value.replace('Z', '+00:00'))
    return result.replace(tzinfo=timezone.utc) if result.tzinfo is None else result.astimezone(timezone.utc)


def _stamp(value):
    return value.astimezone(timezone.utc).isoformat()


def _start(row):
    return _time(row.get('start') or row['time'])


def _end(row):
    return max(_start(row), _time(row.get('end') or row.get('last_seen') or row.get('start') or row['time']))


def _identity(session, state, start):
    return 'timeline-' + hashlib.sha256(f'{session}/{state}/{_stamp(start)}'.encode()).hexdigest()[:24]


def _codes(row):
    evidence = row.get('evidence') or []
    return set(evidence.get('codes', [])) if isinstance(evidence, dict) else set(evidence)


def _state(row):
    stage = row.get('stage', '')
    kind = row.get('kind', '')
    if stage == 'cod_oper' or stage in _NO_SLOTS and 'operation_code' in _codes(row):
        return 'cod'
    if stage in {'expired', 'blocked'} or kind in {'expired', 'blocked'}:
        return 'anomaly'
    if stage == 'booking':
        return 'manual'
    if any(word in stage or word in kind for word in
           ('error', '429', '500', 'timeout', 'unknown', 'rate_limit', 'loading')):
        return 'recovery'
    return 'search'


def _generation(row):
    evidence = row.get('evidence') or {}
    value = evidence.get('browser_generation', 0) if isinstance(evidence, dict) else 0
    return value if isinstance(value, int) else 0


def _order(row):
    # Recognition/check resets precede the availability they discover.
    rank = 0 if not row.get('event') else {'check': 1, 'office': 2, 'booking': 3}.get(row.get('kind'), 1)
    return (_start(row), rank)


def _availability(rows):
    markers = []
    active = None
    for row in sorted(rows, key=_order):
        kind, stage = row.get('kind'), row.get('stage', '')
        reset = (kind in {'check', 'flow_restart', 'finish', 'fatal_error'} or
            kind == 'phase' and row.get('mode') in _GAPS | _TERMINAL or
            kind == 'browser_session_started' and _generation(row) >= 2 or
            kind == 'page' and stage in _SEARCH_STAGES | {'expired', 'blocked', 'server_error', 'rate_limited', 'unknown'})
        if reset:
            active = None
        if not row.get('event') or kind not in {'office', 'booking', 'appointment'}:
            continue
        if active is None:
            when = _start(row)
            session = row.get('session_id', row.get('session', ''))
            active = dict(row, kind='availability', event=True,
                id=_identity(session, 'availability', when), timeline_state='available',
                start=_stamp(when), end=_stamp(when), last_seen=_stamp(when),
                availability_kind='office', office_seen_at=None, booking_seen_at=None,
                offices=[], source_rows=[], restored=False, restored_ranges=[])
            markers.append(active)
        active['source_rows'].append(row)
        if kind == 'office':
            active['office_seen_at'] = active['office_seen_at'] or row.get('start') or row.get('time')
        else:
            active['availability_kind'] = 'booking'
            active['booking_seen_at'] = active['booking_seen_at'] or row.get('start') or row.get('time')
        for office in row.get('offices', []):
            if office not in active['offices']:
                active['offices'].append(office)
    return markers


def _navigation_transients(rows, pages, phases, session):
    context = session.get('context') or {}
    if isinstance(context, str):
        try:
            context = json.loads(context)
        except (ValueError, TypeError):
            context = {}
    historical = str(session.get('session', session.get('session_id', ''))).startswith(('archive-', 'capture-')) or isinstance(context, dict) and context.get('app_version') == 'historical'
    blocked_kinds = {'recovery', 'timeout', 'network_error', 'fatal', 'fatal_error',
        'browser_error', 'manual_required', 'finish', 'handoff', 'pause',
        'server_error', 'rate_limited', 'blocked', 'expired'}
    blocked_modes = _MANUAL | _GAPS | _TERMINAL | {'recovery'}
    blocked_codes = {'http_429', 'http_5xx', 'server_error', 'rate_limited',
        'blocked', 'expired', 'network_error', 'timeout'}
    blocked_times = []
    for row in rows:
        evidence = row.get('evidence') or {}
        status = evidence.get('http_status') if isinstance(evidence, dict) else None
        if (row.get('event') and row.get('kind') in blocked_kinds
            or row.get('kind') == 'browser_session_started' and _generation(row) >= 2
            or isinstance(status, int) and (status == 429 or 500 <= status <= 599)
            or bool(_codes(row) & blocked_codes)):
            blocked_times.append(_start(row))
    blocked_times.sort()
    phase_starts = [_start(row) for row in phases]
    result = {}
    index = 0
    while index < len(pages):
        if pages[index].get('stage') not in {'unknown', 'loading'}:
            index += 1
            continue
        begin = index
        while index < len(pages) and pages[index].get('stage') in {'unknown', 'loading'}:
            index += 1
        if begin == 0 or index == len(pages):
            continue
        before, after = pages[begin-1], pages[index]
        left, right = _start(pages[begin]), _start(after)
        if _state(before) != 'search' or _state(after) != 'search' or (right-left).total_seconds() > 5:
            continue
        phase_index = bisect_right(phase_starts, left)-1
        if not historical:
            if phase_index < 0 or phases[phase_index].get('mode') != 'working':
                continue
            if phase_index+1 < len(phases) and phase_starts[phase_index+1] < right:
                continue
        position = bisect_left(blocked_times, left)
        if position < len(blocked_times) and blocked_times[position] <= right:
            continue
        blocked = False
        for candidate in range(max(0, phase_index), bisect_left(phase_starts, right)):
            phase = phases[candidate]
            if phase.get('mode') not in blocked_modes:
                continue
            phase_end = phase_starts[candidate+1] if candidate+1 < len(phases) else right
            if phase.get('end') and _start(before) > _start(phase) and _start(before) >= _end(phase):
                phase_end = min(phase_end, _end(phase))
            if phase_end > left:
                blocked = True
                break
        if not blocked:
            facts = pages[begin:index]
            result.update((id(row), dict(source_rows=facts, historical=historical)) for row in facts)
    return result


def _append_range(ranges, left, right):
    values = (_stamp(left), _stamp(right))
    if ranges and ranges[-1][1] == values[0]:
        ranges[-1] = (ranges[-1][0], values[1])
    else:
        ranges.append(values)


def _facts(clip, sources, right, seen_ids, seen_stages):
    for row in sources:
        if id(row) not in seen_ids:
            seen_ids.add(id(row))
            clip['source_rows'].append(row)
        stage = row.get('stage')
        if stage and stage not in seen_stages:
            seen_stages.add(stage)
            clip['page_stages'].append(stage)
        for value in (row.get('start'), row.get('last_seen')):
            if value and _time(value) <= right:
                clip['last_seen'] = max(clip['last_seen'], _stamp(_time(value)))


def _run(rows, session, cap=None):
    session_id = session.get('session', session.get('session_id', ''))
    known = bool(session.get('start'))
    start = _time(session['start']) if known else min(_start(row) for row in rows)
    supported_end = max((_end(row) for row in rows if not row.get('event')), default=start)
    if known:
        boundary = session.get('end') or session.get('last_seen')
        end = _time(boundary) if boundary else supported_end
        if not session.get('end'):
            observed_events = [_start(row) for row in rows if row.get('event') and row.get('kind') in {'check', 'office', 'booking', 'appointment', 'browser_session_started'}]
            end = max([end, supported_end] + observed_events)
        if session.get('end') and session.get('reason') not in _CONFIRMED_REASONS:
            end = min(end, supported_end)
    else:
        end = supported_end
    original_end = end
    source_overlap = cap is not None and cap < end
    if cap is not None:
        end = min(end, cap)
    pages = sorted((row for row in rows if row.get('kind') == 'page' and not row.get('event')), key=_start)
    phases = sorted((row for row in rows if row.get('kind') == 'phase' and not row.get('event')), key=_start)
    transients = _navigation_transients(rows, pages, phases, session)
    availability = _availability([row for row in rows if start <= _start(row) <= end])
    markers = []
    resets = []
    forced = {_start(row) for row in availability}
    for row in rows:
        if not row.get('event'):
            continue
        if row.get('kind') in {'office', 'booking', 'appointment'}:
            continue
        if row.get('kind') == 'browser_session_started':
            if _generation(row) < 2:
                continue
            marker = dict(row, timeline_state='restart', source_rows=[row], restored=False)
            forced.add(_start(row)); resets.append(row)
        else:
            marker = dict(row, timeline_state='fatal' if row.get('kind') in {'fatal', 'fatal_error', 'browser_error'} else _state(row), source_rows=[row], restored=False)
        if row.get('kind') in {'finish', 'fatal_error', 'fatal', 'browser_error'}:
            resets.append(row); forced.add(_start(row))
        markers.append(marker)
    boundaries = {start, end} | forced
    for row in pages + phases + resets:
        boundaries.add(_start(row)); boundaries.add(_end(row))
    boundaries = sorted(point for point in boundaries if start <= point <= end)
    page_starts = [_start(row) for row in pages]
    recognized_pages = [row for row in pages if row.get('stage') != 'loading']
    recognized_starts = [_start(row) for row in recognized_pages]
    phase_starts = [_start(row) for row in phases]
    reset_sources = {}
    for row in resets:
        reset_sources.setdefault(_start(row), []).append(row)
    reset_starts = sorted({start} | set(reset_sources) | {
        _start(row) for row in phases if row.get('mode') in _GAPS | _TERMINAL})
    result = []
    seen_ids, seen_stages, seen_transient_groups = set(), set(), set()
    for left, right in zip(boundaries, boundaries[1:]):
        if right <= left:
            continue
        phase_position = bisect_right(phase_starts, left)-1
        phase = phases[phase_position] if phase_position >= 0 else None
        # A gap/new browser/stop invalidates the preceding rendered state.
        reset_at = reset_starts[bisect_right(reset_starts, left)-1]
        page_position = bisect_right(recognized_starts, left)-1
        page = recognized_pages[page_position] if page_position >= 0 and recognized_starts[page_position] >= reset_at else None
        if page is None:
            page_position = bisect_right(page_starts, left)-1
            page = pages[page_position] if page_position >= 0 and page_starts[page_position] >= reset_at else None
        mode = phase.get('mode', '') if phase else ''
        sources = []
        state = None
        stale_phase = bool(phase and phase.get('end') and page
            and _start(page) > _start(phase) and _start(page) >= _end(phase))
        manual_error = bool(page and page.get('stage') in {'expired', 'blocked', 'server_error', 'rate_limited', 'unknown'} and mode == 'manual')
        navigation_transient = bool(page and id(page) in transients)
        if phase and mode in _MANUAL and not stale_phase and not manual_error and (_end(phase) > left or known):
            state = 'manual'; sources = [phase]
        elif phase and mode in _GAPS | _TERMINAL and (page is None or _start(page) <= _start(phase)):
            state = 'gap'; sources = [phase]
        elif phase and _state(phase) == 'cod' and _end(phase) > left and (page is None or _start(page) <= _start(phase)):
            state = 'cod'; sources = [phase]
        elif phase and mode == 'recovery' and _end(phase) > left and (page is None or _start(page) <= _start(phase)) and (page is None or _state(page) not in {'cod', 'anomaly'}):
            state = 'recovery'; sources = [phase] + ([page] if page else [])
        elif page:
            state = 'search' if navigation_transient else _state(page)
            sources = [page]
            if phase and _start(phase) >= reset_at:
                sources.append(phase)
        elif phase and _start(phase) >= reset_at:
            state = 'recovery' if mode == 'recovery' else 'search'; sources = [phase]
        elif reset_at in reset_sources:
            state = 'gap'; sources = reset_sources[reset_at]
        if state is None:
            continue
        actual = sources[0]
        supported = _start(actual) <= left and _end(actual) >= right
        historical_transient = navigation_transient and transients[id(page)]['historical']
        if navigation_transient and not historical_transient:
            supported = True
        if not known and not supported:
            continue
        # Resume after a real gap starts a new clip even if its category matches.
        boundary = left in forced or (result and result[-1]['timeline_state'] == 'gap' and state != 'gap')
        if result and result[-1]['timeline_state'] == state and result[-1]['end'] == _stamp(left) and not boundary:
            clip = result[-1]
        else:
            clip = dict(kind='clip', event=False, session_id=session_id,
                id=_identity(session_id, state, left), start=_stamp(left), end=_stamp(right),
                last_seen=_stamp(_start(actual)), timeline_state=state, stage=actual.get('stage', ''),
                mode=mode, confirmed_end=False, restored=False, restored_ranges=[],
                source_rows=[], page_stages=[], evidence=actual.get('evidence', []), checks=0)
            result.append(clip)
            seen_ids, seen_stages, seen_transient_groups = set(), set(), set()
        clip['end'] = _stamp(right)
        if navigation_transient and state == 'search':
            transient_facts = transients[id(page)]['source_rows']
            if id(transient_facts) not in seen_transient_groups:
                seen_transient_groups.add(id(transient_facts))
                _facts(clip, transient_facts, right, seen_ids, seen_stages)
        _facts(clip, sources, right, seen_ids, seen_stages)
        if (not supported or historical_transient and state == 'search') and state != 'gap':
            clip['restored'] = True
            _append_range(clip['restored_ranges'], left, right)
        if navigation_transient and not historical_transient and state == 'search':
            clip['provisional_reason'] = 'working_navigation_pending'
        if historical_transient and state == 'search':
            clip['restored_reason'] = 'historical_navigation_transient'
            provenance = clip.setdefault('restoration_provenance', [])
            transient_facts = transients[id(page)]['source_rows']
            if provenance and provenance[-1]['end'] == _stamp(left) and provenance[-1]['source_rows'] is transient_facts:
                provenance[-1]['end'] = _stamp(right)
            else:
                provenance.append(dict(start=_stamp(left), end=_stamp(right), reason='historical_navigation_transient', source_rows=transient_facts))
        clip['confirmed_end'] = right < end or bool(known and session.get('end') and session.get('reason') in _CONFIRMED_REASONS)
    # Preserve isolated observations when no session authorizes a duration.
    latest_points = {_start(row): row for row in pages}
    represented = {id(source) for clip in result for source in clip['source_rows']}
    interval_starts = [_time(clip['start']) for clip in result]
    interval_ends = [_time(clip['end']) for clip in result]
    for when, row in latest_points.items():
        position = bisect_right(interval_starts, when) - 1
        covered = position >= 0 and when < interval_ends[position]
        if not start <= when <= end or id(row) in represented or covered:
            continue
        if row.get('stage') == 'loading' and recognized_starts and recognized_starts[0] < when:
            continue
        state = _state(row)
        result.append(dict(kind='clip', event=False, session_id=session_id,
            id=_identity(session_id, state, when), start=_stamp(when), end=_stamp(when),
            last_seen=_stamp(when), timeline_state=state, stage=row.get('stage', ''),
            mode='', confirmed_end=False, restored=False, restored_ranges=[],
            source_rows=[row], page_stages=[row.get('stage', '')], evidence=row.get('evidence', []), checks=0))
    check_times = sorted(_start(row) for row in rows if row.get('kind') == 'check' and row.get('event'))
    for clip in result:
        if source_overlap and clip['end'] == _stamp(end):
            clip['source_overlap'] = True
            clip['original_source_end'] = _stamp(original_end)
            clip['restored'] = True
        clip['checks'] = bisect_left(check_times, _time(clip['end'])) - bisect_left(check_times, _time(clip['start']))
    return result + [row for row in markers + availability if start <= _start(row) <= end]


def build_timeline(rows, sessions=(), *, cache=None):
    """Return serial clips and event markers without mutating source dictionaries.

    Missing sessions retain only supported raw spans. Confirmed run boundaries
    permit last-known-state restoration, explicitly listed in restored_ranges.
    Different sessions are never joined, including overlapping archive sessions.
    Optional caller-owned cache retains at most 128 unchanged run projections;
    cached dictionaries and nested source facts must be treated as immutable.
    """
    groups = {}
    for row in rows:
        if not (row.get('start') or row.get('time')):
            continue
        session_id = row.get('session_id', row.get('session', ''))
        groups.setdefault(session_id, []).append(row)
    metadata = {row.get('session', row.get('session_id', '')): row for row in sessions}
    runs = sorted(groups, key=lambda key: _time(metadata[key]['start']) if key in metadata and metadata[key].get('start') else min(_start(row) for row in groups[key]))
    result = []
    for index, key in enumerate(runs):
        next_key = runs[index+1] if index+1 < len(runs) else None
        cap = (_time(metadata[next_key]['start']) if next_key in metadata and metadata[next_key].get('start') else min(_start(row) for row in groups[next_key])) if next_key is not None else None
        run_metadata = metadata.get(key, {'session': key})
        if cache is None:
            projected = _run(groups[key], run_metadata, cap)
        else:
            signature = json.dumps([groups[key], run_metadata, _stamp(cap) if cap else None],
                sort_keys=True, separators=(',', ':'), default=str)
            cache_key = (key, hashlib.sha256(signature.encode()).hexdigest())
            projected = cache.pop(cache_key, None)
            if projected is None:
                projected = _run(groups[key], run_metadata, cap)
            cache[cache_key] = projected
            while len(cache) > 128:
                del cache[next(iter(cache))]
        result.extend(projected)
    return sorted(result, key=lambda row: (_start(row), bool(row.get('event')), str(row.get('id', ''))))
