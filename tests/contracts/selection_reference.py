"""Synthetic selection goldens referencing existing timeline fixtures, without duplication."""
import json
from pathlib import Path
from app.desktop.statistics_selection import selection_summary

def corpus():
    source=json.loads(Path(__file__).with_name('fixtures').joinpath('timeline.json').read_text())
    cases=[]
    for case in source['cases']:
        rows=case['expected']; history=case['input']['rows']; length=len(rows)
        variants=[('all',list(range(length))),('first',[0] if length else []),('last',[length-1] if length else []),('duplicates',[0,0] if length else []),('empty',[])]
        for name,indices in variants:
            cases.append({'id':case['id']+'-'+name,'timeline_case':case['id'],'indices':indices,'expected':selection_summary([rows[i] for i in indices],history)})
    return {'cases':cases}

if __name__=='__main__':
    Path(__file__).with_name('fixtures').joinpath('selection.json').write_text(json.dumps(corpus(),ensure_ascii=False,indent=2)+'\n')
