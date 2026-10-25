"""Read-only statistics for a union of selected timeline intervals and markers."""
from datetime import datetime, timezone
from bisect import bisect_right
from .statistics import COUNTER_KIND


def instant(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00')).astimezone(timezone.utc)


def intervals_union(intervals):
    result=[]
    for left,right in sorted(intervals):
        if right<=left:continue
        if result and left<=result[-1][1]:result[-1]=(result[-1][0],max(right,result[-1][1]))
        else:result.append((left,right))
    return result


def selection_summary(rows, history=()):
    spans=[];known=[];recovery=[];restored=[];explicit=[]
    for row in rows:
        if row.get('event'):
            explicit.extend(row.get('source_rows') or [row]);continue
        left=instant(row.get('_display_start') or row['start'])
        right=instant(row.get('_display_end') or row.get('end') or row['last_seen'])
        spans.append((left,right))
        gap=row.get('timeline_state')=='gap' or row.get('mode') in {'unobserved','sleep','suspended'}
        if not gap:
            known.append((left,right))
            if row.get('timeline_state')=='recovery' or row.get('kind')!='clip' and row.get('stage') in {'unknown','server_error','rate_limited'}:recovery.append((left,right))
            for start,end in row.get('restored_ranges',[]):
                restored.append((max(left,instant(start)),min(right,instant(end))))
    spans=intervals_union(spans);known=intervals_union(known);starts=[a for a,b in known]
    def contains(time):
        index=bisect_right(starts,time)-1
        return index>=0 and time<known[index][1]
    seconds=lambda pairs:sum((b-a).total_seconds() for a,b in intervals_union(pairs))
    counts={key:0 for key in COUNTER_KIND.values()};counts.update(browser_resets=0,unknown_errors=0,fatal_errors=0)
    seen=set();offices=[]
    def add(row, explicit=False):
        if not row.get('event') or not row.get('counted',True):return
        time=instant(row['start'])
        if not explicit and not contains(time):return
        key=(row.get('session_id'),row.get('kind'),row.get('id'),row['start'])
        if key in seen:return
        seen.add(key);kind=row.get('kind');counter=COUNTER_KIND.get(kind)
        if counter and (kind!='office' or row.get('offices')):counts[counter]+=1
        if kind=='browser_session_started' and row.get('evidence',{}).get('browser_generation',0)>=2:counts['browser_resets']+=1
        if kind in {'fatal','fatal_error','browser_error'}:counts['fatal_errors']+=1
        if kind=='office':offices.extend(row.get('offices',[]))
    for row in history:add(row)
    for row in explicit:add(row,True)
    # Unknown pages have no legacy aggregate counter; count observed entries,
    # never restored span fragments or repeated references to the same page.
    unknown=set();recovery=intervals_union(recovery);recovery_starts=[a for a,b in recovery]
    def failed_at(time):
        index=bisect_right(recovery_starts,time)-1
        return index>=0 and time<recovery[index][1]
    for row in history:
        if row.get('kind')=='page' and row.get('stage')=='unknown' and failed_at(instant(row['start'])):
            unknown.add((row.get('session_id'),row.get('id'),row['start']))
    counts['unknown_errors']=len(unknown)
    if not history:
        counts['checks']=sum(row.get('checks',0) for index,row in enumerate(rows) if not row.get('event') and not any(row.get('id')==previous.get('id') and row.get('start')==previous.get('start') for previous in rows[:index]))
    return dict(seconds=seconds(spans),observed_seconds=seconds(known),gap_seconds=max(0,seconds(spans)-seconds(known)),restored_seconds=seconds(restored),counts=counts,offices=list(dict.fromkeys(offices)))
