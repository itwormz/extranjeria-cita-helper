from app.desktop.statistics_timeline import build_timeline

def replay(case):
    return build_timeline(case['rows'],case.get('sessions',[]))
