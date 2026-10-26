"""Profile-scoped history views; all analytics times use Europe/Madrid."""
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo
from PySide6.QtCore import Qt, QDate, QRectF, Signal, QLocale, QPointF, QEvent, QTimer
from PySide6.QtGui import QCursor, QColor, QPainter, QBrush, QTextCharFormat, QIcon, QTransform, QPen, QPolygonF
from PySide6.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QDateEdit, QStackedWidget, QFrame, QListWidget, QDialog, QCalendarWidget, QToolButton, QGridLayout, QAbstractSpinBox, QToolTip)
from .theme import theme_colors
from .page_stack import CurrentPageStack
from ..assist_flow import LABELS

MADRID=ZoneInfo("Europe/Madrid")


def local_time(value):
    return datetime.fromisoformat(value.replace("Z","+00:00")).astimezone(MADRID)


def stamp(value):
    return local_time(value).strftime("%d.%m.%Y · %H:%M:%S")


def segment_kind(row):
    if row.get("timeline_state") in {"search","cod","recovery","anomaly","manual","gap","available","restart","fatal"}:return row["timeline_state"]
    kind,stage,mode=row.get("kind",""),row.get("stage",""),row.get("mode","")
    if kind=='browser_session_started':return 'restart'
    if kind in ("fatal","fatal_error","browser_error"):return "fatal"
    if kind in ("office","booking","appointment"):return "available"
    if mode in ("manual","paused","pause","stopped","booking_handoff"):return "manual"
    if mode in ("unobserved","sleep","suspended"):return "gap"
    evidence=row.get('evidence',[])
    codes=evidence.get('codes',[]) if isinstance(evidence,dict) else evidence
    if stage=='cod_oper' or (stage in {'applicant_no_slots','request_no_slots','request_no_slots_restart','post_contact_no_slots'} and 'operation_code' in codes):return 'cod'
    if stage in {'expired','blocked'} or kind in {'expired','blocked'}:return 'anomaly'
    if kind=='flow_restart' and isinstance(evidence,dict) and evidence.get('previous_stage') not in {None,'province','request_no_slots_restart','post_contact_no_slots'}:return 'anomaly'
    if any(s in stage or s in kind or s in mode for s in ("error","429","blocked","recovery","timeout","unknown","rate_limit")):return "recovery"
    return "search"


def day_ticks(day):
    """Local-hour tick positions follow the actual 23/24/25-hour day."""
    start=datetime.combine(day,datetime.min.time(),MADRID).astimezone(timezone.utc)
    end=datetime.combine(day+timedelta(days=1),datetime.min.time(),MADRID).astimezone(timezone.utc)
    duration=(end-start).total_seconds()
    ticks=[]
    for hour in (0,6,12,18):
        when=datetime.combine(day,datetime.min.time(),MADRID).replace(hour=hour).astimezone(timezone.utc)
        ticks.append((f'{hour:02}:00',(when-start).total_seconds()/duration))
    return ticks+[('24:00',1)]


def wall_clock_fraction(day,fraction):
    """Project an elapsed-day position onto the shared Madrid clock axis."""
    if fraction>=1:return 1.0
    start=datetime.combine(day,datetime.min.time(),MADRID).astimezone(timezone.utc)
    end=datetime.combine(day+timedelta(days=1),datetime.min.time(),MADRID).astimezone(timezone.utc)
    when=(start+(end-start)*fraction).astimezone(MADRID)
    return (when.hour*3600+when.minute*60+when.second+when.microsecond/1e6)/86400


def wall_clock_ranges(day,left,right):
    """Split at clock changes so intervals never bridge a nonexistent hour
    or become negative during the repeated autumn hour."""
    start=datetime.combine(day,datetime.min.time(),MADRID).astimezone(timezone.utc)
    end=datetime.combine(day+timedelta(days=1),datetime.min.time(),MADRID).astimezone(timezone.utc)
    duration=(end-start).total_seconds()
    edges=[left]
    for hour in range(1,int(duration/3600)):
        moment=start+timedelta(hours=hour)
        if (moment-timedelta(microseconds=1)).astimezone(MADRID).utcoffset()!=moment.astimezone(MADRID).utcoffset():
            fraction=hour*3600/duration
            if left<fraction<right:edges.append(fraction)
    edges.append(right)
    result=[]
    for begin,finish in zip(edges,edges[1:]):
        middle=start+timedelta(seconds=duration*(begin+finish)/2)
        offset=(middle.astimezone(MADRID).utcoffset()-start.astimezone(MADRID).utcoffset()).total_seconds()
        result.append(((begin*duration+offset)/86400,(finish*duration+offset)/86400))
    return result


def timeline_segments(rows,start,days):
    """Clip actual UTC intervals at Madrid midnight, retaining source episode IDs."""
    result=[]
    buckets={}
    limit=start+timedelta(days=days)
    for row in rows:
        begin=local_time(row['start']);finish=local_time(row.get('end') or row['last_seen'])
        last=finish.date()
        if not row['event'] and finish!=begin and finish.time()==datetime.min.time():last-=timedelta(days=1)
        day=max(start,begin.date())
        while day<=last and day<limit:
            buckets.setdefault(day,[]).append(row);day+=timedelta(days=1)
    for offset in range(days):
        day=start+timedelta(days=offset)
        left=datetime.combine(day,datetime.min.time(),MADRID).astimezone(timezone.utc)
        right=datetime.combine(day+timedelta(days=1),datetime.min.time(),MADRID).astimezone(timezone.utc)
        length=(right-left).total_seconds();segments=[]
        for row in buckets.get(day,[]):
            begin=local_time(row["start"]).astimezone(timezone.utc)
            finish=local_time(row.get("end") or row["last_seen"]).astimezone(timezone.utc)
            if row["event"] or begin==finish:
                if not left<=begin<right:continue
            elif finish<=left or begin>=right:continue
            segments.append(dict(row=row,left=max(0,(begin-left).total_seconds()/length),right=min(1,(finish-left).total_seconds()/length),kind=segment_kind(row)))
        # Resolve overlaps before painting. Explicit page evidence outranks the
        # coarse session phase; adjacent equal states become one visual band.
        events=[s for s in segments if s['row']['event']]
        points=[dict(s,observation=True) for s in segments if not s['row']['event'] and s['left']==s['right']]
        spans=[s for s in segments if not s['row']['event'] and s['right']>s['left']]
        edges=sorted({value for s in spans for value in (s['left'],s['right'])})
        priority={'gap':0,'search':1,'manual':2,'cod':3,'recovery':4,'anomaly':4,'restart':1,'fatal':5,'available':1}
        bands=[]
        def state_key(segment):
            source=segment['row']
            if source.get('kind')=='clip':return ('clip',source['id'])
            evidence=source.get('evidence',[])
            codes=evidence.get('codes',[]) if isinstance(evidence,dict) else evidence
            return (segment['kind'],source.get('kind'),source.get('stage'),source.get('mode'),source.get('session_id'),source.get('recognition'),tuple(sorted(codes)),source.get('variant'))
        for begin,end in zip(edges,edges[1:]):
            active=[s for s in spans if s['left']<=begin and s['right']>=end]
            if not active:continue
            winner=max(active,key=lambda s:(priority[s['kind']],s['row']['kind']!='phase',s['row']['start']))
            if bands and state_key(bands[-1])==state_key(winner) and bands[-1]['right']==begin:
                bands[-1]['right']=end
                if winner['row'] not in bands[-1]['rows']:bands[-1]['rows'].append(winner['row'])
            else:bands.append(dict(winner,left=begin,right=end,rows=[winner['row']]))
        result.append((day,bands+points+events))
    return result


def observed_duration(seconds,t):
    seconds=max(0,int(seconds));hours,rest=divmod(seconds,3600);minutes,seconds=divmod(rest,60)
    if hours:return f'{hours} {t("ч")} {minutes} {t("мин")}'
    if minutes:return f'{minutes} {t("мин")} {seconds} {t("с")}' if seconds else f'{minutes} {t("мин")}'
    return f'{seconds} {t("с")}'


class Timeline(QWidget):
    selected=Signal(list)
    def __init__(self,host):
        super().__init__();self.host=host;self.days=[];self.hit=[];self.selection=0;self.selected_key=None
        self.view_start=0.;self.view_end=1.;self.zoom_anchor=.5
        self.hover_position=None;self._tooltip_text=""
        self.drag_origin=None;self.dragging=False;self.selection_rects=[];self.hover_key=None;self.selected_rows=[];self.selected_keys=set();self.multi_select=False;self._drag_mode='pan';self._drag_seed=[];self._anchor_key=None
        self.tooltip_timer=QTimer(self);self.tooltip_timer.setSingleShot(True);self.tooltip_timer.setInterval(350)
        self.tooltip_timer.timeout.connect(self.show_hover_tooltip)
        self.setMinimumHeight(70);self.setFocusPolicy(Qt.FocusPolicy.StrongFocus);self.setMouseTracking(True);self.setCursor(Qt.CursorShape.OpenHandCursor)
        self.zoom_out=QToolButton(self);self.zoom_in=QToolButton(self)
        for i,button in enumerate((self.zoom_out,self.zoom_in)):
            button.setFixedSize(26,24);button.move(i*29,0);button.setProperty('toolbarAction',True)
        self.zoom_out.clicked.connect(lambda:self.zoom(2));self.zoom_in.clicked.connect(lambda:self.zoom(.5));self.update_controls()
    def update_controls(self):
        t=self.host.t
        for button,name,text in [(self.zoom_in,'zoom_in','Приблизить'),(self.zoom_out,'zoom_out','Отдалить')]:
            button.setToolTip(t(text));button.setAccessibleName(t(text))
            if hasattr(self.host,'control_icon'):button.setIcon(self.host.control_icon(name))
            else:button.setText('+' if name=='zoom_in' else '−')
        self.zoom_in.setEnabled(self.view_end-self.view_start>1/48+1e-9);self.zoom_out.setEnabled(self.view_end-self.view_start<1-1e-9)
    @staticmethod
    def row_key(row):return (row.get('event',False),row.get('id'),row.get('kind'),row.get('start'))
    @staticmethod
    def display_rows(day,segment):
        sources=segment.get('rows',[segment['row']])
        if segment['row']['event']:return sources
        if segment['row'].get('kind')=='clip':return [dict(source,_display_start=source['start'],_display_end=source.get('end') or source['last_seen']) for source in sources]
        midnight=datetime.combine(day,datetime.min.time(),MADRID).astimezone(timezone.utc)
        following=datetime.combine(day+timedelta(days=1),datetime.min.time(),MADRID).astimezone(timezone.utc)
        begin=(midnight+(following-midnight)*segment['left']).isoformat();end=(midnight+(following-midnight)*segment['right']).isoformat()
        return [dict(source,_display_start=begin,_display_end=end) for source in sources]
    def set_rows(self,rows,start,days,observed_only=False):
        self.days=timeline_segments(rows,start,days)
        if observed_only:self.days=[(day,segments) for day,segments in self.days if segments]
        self.setFixedHeight(42+len(self.days)*38);self.update_controls();self.update()
    def clock_at(self,x):
        fraction=max(0,min(1,(x-72)/max(1,self.width()-82)))
        return max(0,min(86400,(self.view_start+fraction*(self.view_end-self.view_start))*86400))
    def axis_ticks(self):
        span=(self.view_end-self.view_start)*86400;pixels=max(1,self.width()-82)
        step=(10800 if pixels>=500 else 21600) if span>=18*3600 else next((n for n in (60,300,600,900,1800,3600,7200,10800) if span/n<=pixels/70),21600)
        begin=self.view_start*86400;end=self.view_end*86400
        first=int(begin//step)*step
        return [(value,True) for value in range(first,86401,step) if begin-1e-6<=value<=end+1e-6]
    def zoom(self,factor,anchor=None):
        anchor=self.zoom_anchor if anchor is None else anchor
        old_span=self.view_end-self.view_start
        relative=max(0,min(1,(anchor-self.view_start)/old_span))
        span=max(1/48,min(1,old_span*factor))
        self.view_start=max(0,min(1-span,anchor-span*relative));self.view_end=self.view_start+span
        self.update_controls();self.update()
    def paintEvent(self,event):
        p=QPainter(self);c=theme_colors(self.host.preferences['dark']);p.setRenderHint(QPainter.RenderHint.Antialiasing)
        x=72;w=max(1,self.width()-82);span=self.view_end-self.view_start;self.hit=[];self.selection_rects=[]
        def pixel(fraction):return x+w*(fraction-self.view_start)/span
        def clipped(left,right):return max(left,self.view_start),min(right,self.view_end)
        ticks=self.axis_ticks();p.setPen(QColor(c['muted']))
        for second,label in ticks:
            at=pixel(second/86400)
            if label:p.drawText(QRectF(max(x-20,min(at-20,self.width()-42)),0,40,20),Qt.AlignmentFlag.AlignCenter,f'{second//3600:02}:{second%3600//60:02}')
            p.drawLine(QPointF(at,24 if label else 27),QPointF(at,29))
            grid=QColor(c['line']);grid.setAlpha(60);p.setPen(QPen(grid,.5));p.drawLine(QPointF(at,30),QPointF(at,self.height()-6));p.setPen(QColor(c['muted']))
        for index,(day,segments) in enumerate(self.days):
            y=54+index*38;p.setPen(QColor(c['muted']));p.drawText(QRectF(0,y-10,60,28),Qt.AlignmentFlag.AlignVCenter,day.strftime('%d.%m'))
            p.setPen(QPen(QColor(c['history_gap']),1,Qt.PenStyle.DotLine));p.drawLine(QPointF(x,y+3),QPointF(x+w,y+3));p.setPen(Qt.PenStyle.NoPen)
            covered=sorted((s['left'],s['right']) for s in segments if not s['row']['event'] and s['kind']!='gap');cursor=0;gaps=[]
            for begin,end in covered:
                if begin>cursor:gaps.append((cursor,begin))
                cursor=max(cursor,end)
            if cursor<1:gaps.append((cursor,1))
            midnight=datetime.combine(day,datetime.min.time(),MADRID).astimezone(timezone.utc);following=datetime.combine(day+timedelta(days=1),datetime.min.time(),MADRID).astimezone(timezone.utc)
            for begin,end in gaps:
                gap=dict(kind='phase',mode='unobserved',stage='',start=(midnight+(following-midnight)*begin).isoformat(),end=(midnight+(following-midnight)*end).isoformat(),last_seen=(midnight+(following-midnight)*begin).isoformat(),confirmed_end=False,event=False)
                for left,right in wall_clock_ranges(day,begin,end):
                    left,right=clipped(left,right)
                    if right>left:
                        rect=QRectF(pixel(left),y,pixel(right)-pixel(left),10);self.hit.append((rect,[gap]));self.paint_highlight(p,c,rect,[gap])
            markers=[]
            for segment in segments:
                row=segment['row'];is_event=row['event'];point=segment.get('observation',False)
                if is_event and segment['kind'] not in ('available','fatal','restart') or segment['kind']=='gap':continue
                if is_event or point:
                    left=wall_clock_fraction(day,segment['left']);ranges=[(left,left)]
                else:ranges=wall_clock_ranges(day,segment['left'],segment['right'])
                sources=self.display_rows(day,segment)
                for left,right in ranges:
                    if is_event or point:
                        if not self.view_start<=left<=self.view_end:continue
                    else:
                        left,right=clipped(left,right)
                        if right<=left:continue
                    if is_event:
                        if segment['kind']=='restart' and row.get('evidence',{}).get('browser_generation',0)<2:continue
                        markers.append((pixel(left),row,segment['kind'],sources));continue
                    rect=QRectF(pixel(left)-(1 if point else 0),y,2 if point else pixel(right)-pixel(left),10)
                    p.setBrush(QColor(c['history_'+segment['kind']]))
                    if point:p.drawEllipse(QRectF(rect.left(),y+4,2,2))
                    else:p.drawRect(rect)
                    self.hit.append((rect,sources))
                    if row.get('kind')=='clip':
                        edge=QColor(c['bg']);p.setPen(QPen(edge,1));p.drawLine(rect.topLeft(),rect.bottomLeft());p.drawLine(rect.topRight(),rect.bottomRight());p.setPen(Qt.PenStyle.NoPen)
                    self.paint_highlight(p,c,rect,sources)
            groups=[]
            for at,row,kind,sources in sorted(markers,key=lambda marker:marker[0]):
                group=next((group for group in groups if group[1].get('availability_kind',group[1]['kind'])==row.get('availability_kind',row['kind']) and group[2]==kind and at-group[0]<22),None)
                if group:group[3].extend(sources)
                else:groups.append([at,row,kind,list(sources)])
            for at,row,kind,sources in groups:
                office=row.get('availability_kind',row['kind'])=='office';color=QColor(c['history_office'] if office else c['history_'+kind])
                p.setBrush(color);p.setPen(QPen(color,1))
                center=y-15
                rect=QRectF(at-9,center-9,18,18)
                if office:
                    p.setBrush(Qt.BrushStyle.NoBrush);p.drawRoundedRect(QRectF(at-5,center-7,10,14),1,1)
                    for dx in (-2,2):
                        for dy in (-4,0):p.drawPoint(QPointF(at+dx,center+dy))
                    p.drawLine(QPointF(at-1,center+7),QPointF(at-1,center+4));p.drawLine(QPointF(at-1,center+4),QPointF(at+2,center+4))
                    p.drawLine(QPointF(at,center+7),QPointF(at,y))
                elif kind=='restart':
                    p.drawText(rect,Qt.AlignmentFlag.AlignCenter,'↻')
                elif kind=='available':
                    # Calendar/check follows the same line-icon style as controls.
                    p.setBrush(Qt.BrushStyle.NoBrush);p.drawRoundedRect(QRectF(at-6,center-6,12,12),1,1)
                    p.drawLine(QPointF(at-6,center-2),QPointF(at+6,center-2))
                    p.drawLine(QPointF(at-3,center-8),QPointF(at-3,center-4));p.drawLine(QPointF(at+3,center-8),QPointF(at+3,center-4))
                    p.drawPolyline(QPolygonF([QPointF(at-3,center+1),QPointF(at-1,center+3),QPointF(at+3,center-1)]))
                else:p.drawEllipse(QRectF(at-4,center-4,8,8))
                if len(sources)>1:
                    text=str(len(sources));text_width=p.fontMetrics().horizontalAdvance(text)
                    p.setPen(color);p.drawText(QRectF(at+8,center-8,text_width+1,16),Qt.AlignmentFlag.AlignLeft|Qt.AlignmentFlag.AlignVCenter,text);rect.setRight(at+8+text_width+2)
                if any(self.row_key(source)==self.hover_key for source in sources) or any(self.selection_key(source) in self.selected_keys for source in sources):
                    selected=any(self.selection_key(source) in self.selected_keys for source in sources)
                    p.setBrush(Qt.BrushStyle.NoBrush);p.setPen(QPen(QColor(c['history_selection'] if selected else c['text']),2 if selected else 1));p.drawRoundedRect(rect.adjusted(-2,-2,2,2),3,3)
                p.setPen(Qt.PenStyle.NoPen);self.hit.append((rect,sources))
        for rect in self.selection_rects:
            color=QColor(c['history_selection']);wash=QColor(color);wash.setAlpha(32)
            p.setBrush(wash);p.setPen(QPen(color,2));p.drawRoundedRect(rect,2,2)
            p.fillRect(QRectF(rect.left()-1,rect.top()+2,3,rect.height()-4),color)
            p.fillRect(QRectF(rect.right()-2,rect.top()+2,3,rect.height()-4),color)
        if self.hover_position is not None and 72<=self.hover_position.x()<=self.width()-10:
            at=self.hover_position.x();p.setPen(QPen(QColor(c['text']),1))
            p.drawLine(QPointF(at,29),QPointF(at,self.height()-5))
            second=int(self.clock_at(at));label=f'{second//3600:02}:{second%3600//60:02}:{second%60:02}'
            width=p.fontMetrics().horizontalAdvance(label)+12;left=max(72,min(at-width/2,self.width()-10-width))
            p.setBrush(QColor(c['text']));p.setPen(Qt.PenStyle.NoPen);p.drawRoundedRect(QRectF(left,0,width,23),3,3)
            p.setPen(QColor(c['bg']));p.drawText(QRectF(left,0,width,23),Qt.AlignmentFlag.AlignCenter,label)
        p.end()
    @classmethod
    def selection_key(cls,row):
        return (cls.row_key(row),None if row.get('kind')=='clip' else row.get('_display_start'))
    def set_selection(self,rows,emit=False):
        unique={self.selection_key(row):row for row in rows}
        if emit and set(unique)==self.selected_keys:return
        self.selected_rows=list(unique.values());self.selected_keys=set(unique)
        self.selected_key=self.selection_key(self.selected_rows[0]) if self.selected_rows else None
        if not self.selected_rows:self._anchor_key=None
        self.update()
        if emit:self.selected.emit(self.selected_rows)
    def rows_at(self,position):
        return next((rows for rect,rows in reversed(self.hit) if rect.adjusted(-2,-2,2,2).contains(position)),[])
    def paint_highlight(self,p,c,rect,sources):
        if any(self.row_key(source)==self.hover_key for source in sources):
            wash=QColor(c['text']);wash.setAlpha(35);p.fillRect(rect.adjusted(0,-2,0,2),wash)
            p.setPen(QPen(QColor(c['text']),1));p.setBrush(Qt.BrushStyle.NoBrush);p.drawRect(rect.adjusted(-1,-2,1,2));p.setPen(Qt.PenStyle.NoPen)
        if any(self.selection_key(source) in self.selected_keys or self.selection_key(source)==self.selected_key for source in sources):
            self.selection_rects.append(rect.adjusted(-1,-3,1,3))
    def select_drag_range(self,start,end):
        left,right=sorted((start.x(),end.x()));first,last=sorted((int((start.y()-30)//38),int((end.y()-30)//38)));top=30+first*38;bottom=30+(last+1)*38-1
        rows=list(self._drag_seed)
        for rect,sources in self.hit:
            if rect.right()>=left and rect.left()<=right and rect.bottom()>=top-2 and rect.top()<=bottom+2:rows.extend(sources)
        self.set_selection(rows,emit=True)
    def hover_text(self,position):
        seconds=int(self.clock_at(position.x()));clock=f'{seconds//3600:02}:{seconds%3600//60:02}:{seconds%60:02}'
        matched=next((rows for rect,rows in reversed(self.hit) if rect.adjusted(-2,-2,2,2).contains(position)),None)
        index=int((position.y()-30)//38)
        if position.x()<72 or not matched and not 0<=index<len(self.days):return ''
        self.zoom_anchor=self.clock_at(position.x())/86400
        day=self.days[index][0] if 0<=index<len(self.days) else local_time(matched[0].get('_display_start',matched[0]['start'])).date()
        text=f'{day:%d.%m.%Y} · {clock}'
        local=datetime.combine(day,datetime.min.time(),MADRID)+timedelta(seconds=seconds)
        if local.astimezone(timezone.utc).astimezone(MADRID).replace(tzinfo=None)!=local.replace(tzinfo=None):text+=' · '+self.host.t('Переход часов')
        elif local.replace(fold=0).utcoffset()!=local.replace(fold=1).utcoffset():text+=' · CEST / CET'
        if matched:
            row=matched[0];kind=segment_kind(row)
            title={'search':'Поиск','cod':'Нет сит · Cod. Oper.','recovery':'Сбой сайта','anomaly':'Нарушение сессии','manual':'Ручное управление','gap':'Нет наблюдений'}.get(kind,'Поиск') if row.get('kind')=='clip' else 'Выбор даты' if row.get('availability_kind')=='booking' else 'Офисы' if row.get('availability_kind')=='office' else 'Новая сессия' if kind=='restart' else 'Офисы' if row['kind']=='office' else 'Выбор даты' if row['kind']=='booking' else 'Нет сит · Cod. Oper.' if kind=='cod' else 'Нет наблюдений' if kind=='gap' else LABELS.get(row.get('stage'),(self.host.t('Поиск'),))[0]
            text+='\n'+self.host.t(title)
            if row['event']:
                times=[local_time(source['start']).strftime('%H:%M:%S') for source in matched]
                text+=' · '+(', '.join(times))
                if len(matched)>1:text+='\n'+self.host.t('Событий: {count}').format(count=len(matched))
            else:
                begin=min(local_time(r.get('_display_start',r['start'])).astimezone(timezone.utc) for r in matched)
                end=max(local_time(r.get('_display_end') or r.get('end') or r['last_seen']).astimezone(timezone.utc) for r in matched)
                if end>begin:
                    fmt='%d.%m %H:%M:%S' if begin.astimezone(MADRID).date()!=end.astimezone(MADRID).date() else '%H:%M:%S'
                    text+='\n'+begin.astimezone(MADRID).strftime(fmt)+'–'+end.astimezone(MADRID).strftime(fmt)+' · '+observed_duration((end-begin).total_seconds(),self.host.t)
            if kind=='restart' and isinstance(row.get('evidence'),dict):
                previous=row['evidence'].get('previous_stage');text+='\n'+self.host.t(LABELS.get(previous,('Новая сессия',))[0])
            if row['kind']=='office' or row.get('kind')=='availability':text+='\n'+'\n'.join(dict.fromkeys(o for source in matched for o in source.get('offices',[])))
            text+='\n'+self.clip_summary(row) if self.clip_summary(row) else ''
        return text
    def clip_summary(self,row):
        t=self.host.t;parts=[]
        if row.get('restored'):parts.append(t('Восстановлено по журналам запуска'))
        if row.get('source_overlap'):parts.append(t('Перекрывающиеся запуски'))
        if row.get('checks'):parts.append(t('Проверок: {count}').format(count=row['checks']))
        if row.get('kind')=='availability':
            if row.get('office_seen_at'):parts.append(t('Офисы')+': '+stamp(row['office_seen_at']))
            if row.get('booking_seen_at'):parts.append(t('Выбор даты')+': '+stamp(row['booking_seen_at']))
        elif row.get('page_stages'):
            labels=list(dict.fromkeys(t(LABELS.get(stage,(stage,))[0]) for stage in row['page_stages']))
            parts.append(' → '.join(labels[:5])+(' …' if len(labels)>5 else ''))
        return '\n'.join(parts)
    def show_hover_tooltip(self):
        if self.hover_position is not None and self._tooltip_text:
            QToolTip.showText(self.mapToGlobal(self.hover_position.toPoint()),self._tooltip_text,self)
    def mouseMoveEvent(self,event):
        if self.drag_origin is not None and event.buttons() & Qt.MouseButton.LeftButton:
            start,view_start,span=self.drag_origin;delta=event.position().x()-start.x()
            if abs(delta)>=4 or self.dragging:
                self.dragging=True
                if self._drag_mode=='select':
                    self.setCursor(Qt.CursorShape.CrossCursor);self.select_drag_range(start,event.position())
                else:
                    self.setCursor(Qt.CursorShape.ClosedHandCursor)
                    self.view_start=max(0,min(1-span,view_start-delta/max(1,self.width()-82)*span));self.view_end=self.view_start+span
                self.tooltip_timer.stop();QToolTip.hideText();self._tooltip_text="";self.hover_position=None;self.hover_key=None;self.update();event.accept();return
        self.hover_position=event.position()
        hovered=next((rows for rect,rows in reversed(self.hit) if rect.adjusted(-3,-3,3,3).contains(event.position())),[])
        self.hover_key=self.row_key(hovered[0]) if hovered else None
        text=self.hover_text(event.position())
        if text!=self._tooltip_text:
            self._tooltip_text=text;self.tooltip_timer.start();QToolTip.hideText()
        self.update();super().mouseMoveEvent(event)
    def leaveEvent(self,event):
        self.tooltip_timer.stop();self.hover_position=None;self.hover_key=None;self._tooltip_text='';QToolTip.hideText();self.update();super().leaveEvent(event)
    def mousePressEvent(self,event):
        if event.button()==Qt.MouseButton.LeftButton and event.position().x()>=72:
            self.setFocus(Qt.FocusReason.MouseFocusReason)
            self.drag_origin=(event.position(),self.view_start,self.view_end-self.view_start);self.dragging=False
            modifiers=event.modifiers();self._additive=bool(modifiers & (Qt.KeyboardModifier.ControlModifier|Qt.KeyboardModifier.MetaModifier)) or self.multi_select
            self._drag_seed=list(self.selected_rows) if self._additive else []
            self._drag_mode='select' if self.rows_at(event.position()) or self.multi_select else 'pan'
            if modifiers & Qt.KeyboardModifier.AltModifier:self._drag_mode='pan'
            self.zoom_anchor=self.clock_at(event.position().x())/86400;event.accept()
        else:super().mousePressEvent(event)
    def mouseReleaseEvent(self,event):
        if event.button()==Qt.MouseButton.LeftButton and self.drag_origin is not None:
            dragged=self.dragging;self.drag_origin=None;self.dragging=False;self.setCursor(Qt.CursorShape.OpenHandCursor)
            if not dragged:
                rows=self.rows_at(event.position())
                if event.modifiers() & Qt.KeyboardModifier.ShiftModifier and self._anchor_key is not None:
                    anchor=next((rect.center() for rect,group in self.hit if any(self.selection_key(source)==self._anchor_key for source in group)),event.position())
                    self.select_drag_range(anchor,event.position());event.accept();return
                if self._additive:
                    keys={self.selection_key(row) for row in rows};remaining=[row for row in self.selected_rows if self.selection_key(row) not in keys]
                    rows=remaining if keys and keys<=self.selected_keys else remaining+rows
                self.set_selection(rows,emit=True)
                if rows:self._anchor_key=self.selection_key(rows[-1])
            event.accept()
        else:super().mouseReleaseEvent(event)
    def mouseDoubleClickEvent(self,event):
        self.drag_origin=None;self.dragging=False;self.setCursor(Qt.CursorShape.OpenHandCursor)
        if event.position().y()<30:self.view_start=0.;self.view_end=1.;self.update_controls();self.update()
        else:self.zoom((1/48)/(self.view_end-self.view_start),anchor=self.clock_at(event.position().x())/86400)
    def wheelEvent(self,event):
        pixels=event.pixelDelta();angles=event.angleDelta()
        if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
            delta=pixels.y() if not pixels.isNull() else angles.y()/4
            if delta:self.zoom(2**(-delta/120),anchor=self.clock_at(event.position().x())/86400)
            event.accept()
        elif event.modifiers() & Qt.KeyboardModifier.ShiftModifier or abs(pixels.x() if not pixels.isNull() else angles.x())>abs(pixels.y() if not pixels.isNull() else angles.y()):
            delta=(pixels.x() or pixels.y()) if not pixels.isNull() else (angles.x() or angles.y())/4
            span=self.view_end-self.view_start
            self.view_start=max(0,min(1-span,self.view_start-delta/max(1,self.width()-82)*span))
            self.view_end=self.view_start+span;self.update();event.accept()
        else:event.ignore()
    def event(self,event):
        if event.type()==QEvent.Type.NativeGesture:
            if event.gestureType()==Qt.NativeGestureType.ZoomNativeGesture:
                self.zoom(1/max(.05,1+event.value()),anchor=self.clock_at(event.position().x())/86400);event.accept();return True
            if event.gestureType()==Qt.NativeGestureType.SmartZoomNativeGesture:
                self.view_start=0.;self.view_end=1.;self.update_controls();self.update();event.accept();return True
        return super().event(event)
    def keyPressEvent(self,event):
        if event.key()==Qt.Key.Key_Escape:
            self.set_selection([],emit=True);event.accept()
        elif event.key()==Qt.Key.Key_0:self.view_start=0.;self.view_end=1.;self.update_controls();self.update();event.accept()
        elif event.key() in (Qt.Key.Key_Right,Qt.Key.Key_Left,Qt.Key.Key_Return,Qt.Key.Key_Space) and self.hit:
            if event.key()==Qt.Key.Key_Right:self.selection=(self.selection+1)%len(self.hit)
            if event.key()==Qt.Key.Key_Left:self.selection=(self.selection-1)%len(self.hit)
            self.set_selection(self.hit[self.selection][1],emit=True);event.accept()
        else:super().keyPressEvent(event)


class OfficeCountButton(QPushButton):
    def __init__(self,appearances,t,parent=None):
        self.appearances=sorted(appearances,key=lambda r:r['time'],reverse=True);self.t=t
        count=len(appearances)
        source='Предложен 1 раз' if count==1 else 'Предложен {count} раза' if 2<=count%10<=4 and not 12<=count%100<=14 else 'Предложен {count} раз'
        super().__init__(t(source).format(count=count),parent)
        self.setObjectName('officeCount');self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.popup=None;self.preview_labels=[];self.full_list=None;self.popup_interactive=False;self.hover_suppressed=False
        self.hover_timer=QTimer(self);self.hover_timer.setInterval(120);self.hover_timer.timeout.connect(self.check_hover)
        self.clicked.connect(lambda checked=False:self.open_popup(interactive=True))
    def enterEvent(self,event):
        if not self.hover_suppressed and self.rect().contains(self.mapFromGlobal(QCursor.pos())):self.open_popup(interactive=False)
        super().enterEvent(event)
    def eventFilter(self,watched,event):
        if watched is self.popup:
            if event.type()==QEvent.Type.KeyPress and event.key()==Qt.Key.Key_Escape:
                self.popup.hide();return True
            if event.type()==QEvent.Type.Hide and self.popup_interactive:
                self.hover_suppressed=True;self.hover_timer.start()
        return super().eventFilter(watched,event)
    def check_hover(self):
        if self.hover_suppressed:
            if not self.rect().contains(self.mapFromGlobal(QCursor.pos())):
                self.hover_suppressed=False;self.hover_timer.stop()
            return
        if not self.popup or not self.popup.isVisible():self.hover_timer.stop();return
        point=QCursor.pos()
        if not self.rect().contains(self.mapFromGlobal(point)) and not self.popup.rect().contains(self.popup.mapFromGlobal(point)):
            self.popup.hide();self.hover_timer.stop()
    def open_popup(self,interactive=True):
        if self.popup and self.popup.isVisible():
            if self.popup_interactive or not interactive:return
            self.popup.hide()
        if self.popup:self.popup.deleteLater()
        self.hover_timer.stop();self.popup_interactive=interactive;self.hover_suppressed=False
        self.popup=QFrame(self,Qt.WindowType.Popup if interactive else Qt.WindowType.ToolTip);self.popup.setObjectName('officePopup');self.popup.installEventFilter(self);layout=QVBoxLayout(self.popup)
        if not interactive:self.popup.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        layout.addWidget(QLabel(self.t('Появления офиса')+''));self.preview_labels=[]
        for row in self.appearances[:4]:
            label=QLabel(stamp(row['time']));self.preview_labels.append(label);layout.addWidget(label)
        all_button=QPushButton(self.t('Все появления')+f' · {len(self.appearances)}');all_button.clicked.connect(self.show_all);layout.addWidget(all_button)
        self.popup.adjustSize();position=self.mapToGlobal(self.rect().bottomLeft());bounds=self.window().frameGeometry()
        position.setX(max(bounds.left(),min(position.x(),bounds.right()-self.popup.width())))
        position.setY(max(bounds.top(),min(position.y(),bounds.bottom()-self.popup.height())))
        self.popup.move(position);self.popup.show()
        if not interactive:self.hover_timer.start()
    def show_all(self):
        self.hover_timer.stop()
        if self.popup:self.popup.hide()
        self.full_dialog=QDialog(self.window());self.full_dialog.setWindowTitle(self.t('Все появления')+'');layout=QVBoxLayout(self.full_dialog)
        self.full_list=QListWidget();self.full_list.addItems([stamp(row['time']) for row in self.appearances]);layout.addWidget(self.full_list)
        close=QPushButton(self.t('Закрыть'));close.clicked.connect(self.full_dialog.accept);layout.addWidget(close)
        self.full_dialog.resize(360,360);self.full_dialog.show()


class StatisticsHistory(QWidget):
    def __init__(self,host,store):
        super().__init__();self.host=host;self.store=store;self.period='all';self.profile_id=None;self.office_rows=[];self._scope=None;self._office_signature=None;self.rows=[];self.raw_rows=[];self.bounds=None;self.baseline=False;self.selected_rows=[]
        layout=QVBoxLayout(self);layout.setContentsMargins(0,0,0,0);layout.setSpacing(12)
        tabs=QHBoxLayout();self.history_button=QPushButton();self.offices_button=QPushButton()
        for button in (self.history_button,self.offices_button):button.setObjectName('historyTab');button.setCheckable(True);tabs.addWidget(button)
        tabs.addStretch();self.header=tabs;layout.addLayout(tabs);self.stack=CurrentPageStack();layout.addWidget(self.stack)
        history=QWidget();body=QVBoxLayout(history);body.setContentsMargins(0,0,0,0)
        self.custom_range=None;self._render_signature=None;self._data_signature=None
        self.period_button=QPushButton();self.period_button.setObjectName('historyPeriod');self.header.addWidget(self.period_button)
        self.period_button.clicked.connect(self.open_period)
        self.legend=QLabel();self.legend.setWordWrap(True)
        legend_line=QHBoxLayout();legend_line.setSpacing(8);legend_line.addWidget(self.legend,1)
        self.legend_button=QToolButton();self.legend_button.setProperty('toolbarAction',True);self.legend_button.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon);self.legend_button.clicked.connect(self.open_legend);legend_line.addWidget(self.legend_button,0,Qt.AlignmentFlag.AlignTop);body.addLayout(legend_line)
        self.selection_button=QToolButton();self.selection_button.setProperty('toolbarAction',True);self.selection_button.setToolButtonStyle(Qt.ToolButtonStyle.ToolButtonTextBesideIcon);self.selection_button.setCheckable(True);legend_line.addWidget(self.selection_button)
        self.timeline=Timeline(host);body.addWidget(self.timeline)
        self.selection_button.toggled.connect(lambda checked:setattr(self.timeline,'multi_select',checked))
        self.detail_card=QFrame();self.detail_card.setObjectName('historyDetail');detail_layout=QHBoxLayout(self.detail_card);detail_layout.setContentsMargins(12,10,12,10);detail_layout.setSpacing(10)
        self.detail_icon=QLabel();self.detail_icon.setFixedSize(20,20);detail_layout.addWidget(self.detail_icon,0,Qt.AlignmentFlag.AlignTop)
        self.detail=QLabel();self.detail.setWordWrap(True);self.detail.setTextFormat(Qt.TextFormat.RichText);detail_layout.addWidget(self.detail,1);body.addWidget(self.detail_card)
        self.empty=QLabel();self.empty.setWordWrap(True);body.addWidget(self.empty);self.stack.addWidget(history)
        self.offices=QWidget();self.offices_layout=QVBoxLayout(self.offices);self.offices_layout.setContentsMargins(0,0,0,0);self.stack.addWidget(self.offices)
        self.history_button.clicked.connect(lambda:self.select_tab(0));self.offices_button.clicked.connect(lambda:self.select_tab(1));self.select_tab(0)
        self.timeline.selected.connect(self.show_details)
    def select_tab(self,index):
        self.stack.setCurrentIndex(index);self.history_button.setChecked(index==0);self.offices_button.setChecked(index==1)
    def open_legend(self):
        if getattr(self,'legend_popup',None):self.legend_popup.deleteLater()
        t=self.host.t;c=theme_colors(self.host.preferences['dark'])
        self.legend_popup=QFrame(self,Qt.WindowType.Popup);self.legend_popup.setObjectName('officePopup');layout=QVBoxLayout(self.legend_popup)
        items=[('search','Поиск','Поиск'),('cod','Cod. Oper.','Нет сит · Cod. Oper.'),('recovery','Сбой сайта','HTTP 429, 500, таймаут или неизвестная ошибка'),('anomaly','Сбой сессии','Сессия истекла или доступ ограничен'),('office','▥','Офисы'),('available','▦','Выбор даты'),('restart','↻','Новая сессия'),('manual','■','Ручное управление'),('fatal','●','Остановка с ошибкой'),('gap','·','Нет наблюдений')]
        for key,mark,label in items:
            text=QLabel(f'<span style="color:{c["history_"+key]}">{t(mark)}</span> · {t(label)}');text.setWordWrap(True);layout.addWidget(text)
        self.legend_popup.setFixedWidth(min(390,max(240,self.window().width()-32)));self.legend_popup.adjustSize()
        position=self.legend_button.mapToGlobal(self.legend_button.rect().bottomRight());position.setX(position.x()-self.legend_popup.width())
        screen=self.screen().availableGeometry();position.setX(max(screen.left(),min(position.x(),screen.right()-self.legend_popup.width())))
        position.setY(max(screen.top(),min(position.y(),screen.bottom()-self.legend_popup.height())))
        self.legend_popup.move(position);self.legend_popup.show()
    def open_period(self):
        t=self.host.t
        self.period_dialog=QDialog(self.window());self.period_dialog.setWindowTitle(t('Период'));self.period_dialog.setWindowModality(Qt.WindowModality.WindowModal);self.period_dialog.setMinimumWidth(360);layout=QGridLayout(self.period_dialog);layout.setContentsMargins(20,20,20,20);layout.setHorizontalSpacing(12);layout.setVerticalSpacing(12)
        today=datetime.now(MADRID).date()
        begin,end=self.custom_range or (local_time(self.bounds.get('start') or self.bounds['end']).date() if self.bounds else today,local_time(self.bounds['end']).date() if self.bounds else today)
        self.period_from=QDateEdit(QDate(begin));self.period_to=QDateEdit(QDate(end))
        earliest=min(begin,today,local_time(self.bounds.get('start') or self.bounds['end']).date()) if self.bounds else today
        latest=max(end,today,local_time(self.bounds['end']).date()) if self.bounds else today
        locale=QLocale({'ru':'ru_RU','es':'es_ES'}.get(self.host.preferences['language'],'en_GB'))
        for index,(caption,field) in enumerate((('С',self.period_from),('По',self.period_to))):
            label=QLabel(t(caption));label.setBuddy(field);layout.addWidget(label,index,0);layout.addWidget(field,index,1)
            field.setDateRange(QDate(earliest),QDate(latest));field.setObjectName('historyRangeDate');field.setMinimumHeight(40);field.setAccessibleName(t(caption));field.setDisplayFormat('dd.MM.yyyy');field.setCalendarPopup(True);field.setLocale(locale);field.setButtonSymbols(QAbstractSpinBox.ButtonSymbols.NoButtons)
            calendar=field.calendarWidget();calendar.setObjectName('appointmentCalendar');calendar.setLocale(locale);calendar.setGridVisible(False);calendar.setVerticalHeaderFormat(QCalendarWidget.VerticalHeaderFormat.NoVerticalHeader);calendar.setFirstDayOfWeek(Qt.DayOfWeek.Monday)
            normal=QTextCharFormat();normal.setForeground(QColor(theme_colors(self.host.preferences['dark'])['text']))
            for day in Qt.DayOfWeek:calendar.setWeekdayTextFormat(day,normal)
            if hasattr(self.host,'control_icon'):
                icon=self.host.control_icon('back')
                for name,reverse in [('qt_calendar_prevmonth',False),('qt_calendar_nextmonth',True)]:
                    control=calendar.findChild(QToolButton,name)
                    if control:control.setIcon(QIcon(icon.pixmap(18,18).transformed(QTransform().rotate(180))) if reverse else icon)
            field.dateChanged.connect(self.validate_period)
        self.period_error=QLabel(t('Начало должно быть не позже окончания'));self.period_error.setWordWrap(True);layout.addWidget(self.period_error,2,0,1,2)
        actions=QHBoxLayout();all_days=QPushButton(t('Все дни'));all_days.clicked.connect(lambda:self.apply_period(None));actions.addWidget(all_days)
        self.period_apply=QPushButton(t('Применить'));self.period_apply.setProperty('primary',True);self.period_apply.clicked.connect(lambda:self.apply_period((self.period_from.date().toPython(),self.period_to.date().toPython())));actions.addWidget(self.period_apply);layout.addLayout(actions,3,0,1,2);layout.setColumnStretch(1,1)
        self.validate_period();self.period_dialog.show();self.period_apply.setFocus()
        for field in (self.period_from,self.period_to):field.lineEdit().deselect()
    def validate_period(self,*_):
        valid=self.period_from.date()<=self.period_to.date()
        if hasattr(self,'period_apply'):self.period_apply.setEnabled(valid);self.period_error.setVisible(not valid)
    def apply_period(self,value):
        self.custom_range=value;self.period_dialog.accept();self.render_timeline()
    def refresh(self,profile_id,period,baseline=False):
        t=self.host.t;self.history_button.setText(t('История поиска'));self.offices_button.setText(t('Офисы'))
        scope=(profile_id,period,datetime.now(MADRID).date() if period=='today' else None)
        if scope!=self._scope:
            self.custom_range=None;self._scope=scope;self._data_signature=None;self.timeline.view_start=0.;self.timeline.view_end=1.;self.selected_rows=[];self.timeline.set_selection([])
        self.profile_id=profile_id;self.period=period;self.baseline=baseline;self.period_button.setEnabled(period=='all')
        path=getattr(self.store,'db_path',None)
        revision=tuple((p.stat().st_mtime_ns,p.stat().st_size) if p.exists() else None for p in (path,path.with_name(path.name+'-wal'))) if path else None
        signature=(scope,revision)
        if revision is None or signature!=self._data_signature:
            self.bounds=getattr(self.store,'timeline_bounds',self.store.history_bounds)(profile_id,period=period)
            if not self.bounds or not self.bounds.get('end'):self.bounds=None
            self.office_rows=self.store.office_history(profile_id,period=period);self._data_signature=signature;self._render_signature=None
        self.offices_button.setText(t('Офисы')+' · '+str(len(self.office_rows)))
        if hasattr(self.host,'control_icon'):self.period_button.setIcon(self.host.control_icon('calendar_days'))
        c=theme_colors(self.host.preferences['dark'])
        legend=[('search','Поиск'),('cod','Cod. Oper.'),('recovery','Сбой сайта'),('anomaly','Сбой сессии'),('office','Офисы'),('available','Даты')]
        symbols={'office':'▥','available':'▦'}
        self.legend.setText(' &nbsp; '.join(f'<span style="color:{c["history_"+key]}">{symbols.get(key,"■")}</span> {t(name)}' for key,name in legend))
        self.legend_button.setText(t('Обозначения'));self.selection_button.setText(t('Выбрать несколько'));self.selection_button.setToolTip(t('⌘/Ctrl-клик — добавить участок; протягивание — выбрать диапазон'))
        if hasattr(self.host,'control_icon'):
            self.legend_button.setIcon(self.host.control_icon('info'));self.selection_button.setIcon(self.host.control_icon('copy'))
        self.timeline.setAccessibleName(t('Поиск · Cod. Oper. · Восстановление · Доступность · Нет наблюдений · Ручное управление')+'')
        signature=(scope,self.host.preferences['language'],repr(self.office_rows),baseline)
        if signature==self._office_signature:
            self.render_timeline();return
        self._office_signature=signature
        while self.offices_layout.count():
            item=self.offices_layout.takeAt(0)
            if item.widget():item.widget().deleteLater()
        for row in self.office_rows:
            frame=QFrame();frame.setObjectName('statisticRow');line=QHBoxLayout(frame);words=QVBoxLayout();name_text,_,address_text=row['label'].partition(',');name=QLabel(name_text);name.setTextFormat(Qt.TextFormat.PlainText);name.setWordWrap(True);words.addWidget(name)
            if address_text:
                address=QLabel(address_text.strip());address.setTextFormat(Qt.TextFormat.PlainText);address.setProperty('muted',True);address.setWordWrap(True);words.addWidget(address)
            last=QLabel(stamp(row['last_seen']) if row['last_seen'] else t('Дата неизвестна'));last.setProperty('muted',True);words.addWidget(last);line.addLayout(words,1)
            button=OfficeCountButton(row['appearances'],t) if row['appearances'] else QPushButton(t('Нет истории появлений'))
            if not row['appearances']:button.setEnabled(False)
            if row['appearances'] and hasattr(self.host,'control_icon'):button.setIcon(self.host.control_icon('clock'))
            line.addWidget(button);self.offices_layout.addWidget(frame)
        if not self.office_rows:
            label=QLabel(t('Старые итоги сохранены. Подробная история отсутствует.') if baseline else t('Офисы пока не появлялись'));label.setWordWrap(True);self.offices_layout.addWidget(label)
        self.render_timeline()
    def render_timeline(self,*_):
        if self.profile_id is None:return
        today=datetime.now(MADRID).date()
        if self.period=='today':start=end=today
        elif self.custom_range:start,end=self.custom_range
        elif self.bounds:start=local_time(self.bounds.get('start') or self.bounds['end']).date();end=local_time(self.bounds['end']).date()
        else:start=end=today
        days=(end-start).days+1
        t=self.host.t
        label=t('Сегодня') if self.period=='today' else f'{start:%d.%m.%Y}–{end:%d.%m.%Y}' if self.custom_range else t('Все дни')
        self.period_button.setText(t('Период')+' · '+label);self.period_button.setToolTip(f'{start:%d.%m.%Y}–{end:%d.%m.%Y}')
        signature=(self._data_signature,self.custom_range,self.host.preferences['language'],self.host.preferences['dark'])
        if self._render_signature==signature:return
        self._render_signature=signature
        lower=datetime.combine(start,datetime.min.time(),MADRID)
        upper=datetime.combine(end+timedelta(days=1),datetime.min.time(),MADRID)
        self.rows=getattr(self.store,'timeline_history',self.store.history)(self.profile_id,period=self.period,start=lower,end=upper)
        self.raw_rows=self.store.history(self.profile_id,period='all',start=min([lower]+[local_time(row['start']) for row in self.rows]),end=max([upper]+[local_time(row.get('end') or row['last_seen'])+timedelta(microseconds=1) for row in self.rows]))
        self.timeline.set_rows(self.rows,start,days,observed_only=self.period=='all' and self.custom_range is None);self.timeline.setVisible(bool(self.rows or self.bounds))
        selected_keys={Timeline.selection_key(row) for row in self.selected_rows}
        selected=[]
        for day,segments in self.timeline.days:
            for segment in segments:
                group=Timeline.display_rows(day,segment)
                selected.extend(row for row in group if Timeline.selection_key(row) in selected_keys)
        # Empty intervals are generated by painting; retain them only while no
        # newly observed interval overlaps the selected empty range.
        for row in self.selected_rows:
            if segment_kind(row)=='gap' and lower.astimezone(timezone.utc)<local_time(row.get('end') or row['last_seen']).astimezone(timezone.utc) and local_time(row['start']).astimezone(timezone.utc)<upper.astimezone(timezone.utc) and not any(not other.get('event') and segment_kind(other)!='gap' and local_time(other['start']).astimezone(timezone.utc)<local_time(row.get('end') or row['last_seen']).astimezone(timezone.utc) and local_time(other.get('end') or other['last_seen']).astimezone(timezone.utc)>local_time(row['start']).astimezone(timezone.utc) for other in self.rows):selected.append(row)
        self.show_details(selected)
        t=self.host.t
        self.empty.setText('' if self.rows else t('Нет наблюдений в выбранном диапазоне') if self.bounds else t('Старые итоги сохранены. Подробная история отсутствует.') if self.baseline else t('Пока нет истории поиска'))
    def show_details(self,rows):
        from html import escape
        from .statistics_selection import selection_summary,instant
        self.timeline.set_selection(rows);rows=self.timeline.selected_rows
        self.selected_rows=list(rows);self.detail_card.setVisible(bool(rows))
        if not rows:self.detail.clear();return
        t=self.host.t;c=theme_colors(self.host.preferences['dark']);row=rows[0];kind=segment_kind(row)
        summary=selection_summary(rows,self.raw_rows);many=len(rows)>1
        names={'search':'Поиск','cod':'Нет сит · Cod. Oper.','gap':'Нет наблюдений','manual':'Ручное управление','recovery':'Сбой сайта','anomaly':'Нарушение сессии','restart':'Новая сессия','available':'Доступность','fatal':'Остановка с ошибкой'}
        label=t('Выбрано: {count}').format(count=len(rows)) if many else t('Выбор даты') if row.get('availability_kind',row['kind'])=='booking' else t('Офисы') if row.get('availability_kind',row['kind'])=='office' else t(names[kind])
        status=row.get('evidence',{}).get('http_status') if isinstance(row.get('evidence'),dict) else None
        if status and not many:label+=f' · HTTP {status}'
        start=min(instant(source.get('_display_start') or source['start']) for source in rows).astimezone(MADRID)
        finish=max(instant(source.get('_display_end') or source.get('end') or source['last_seen']) for source in rows).astimezone(MADRID)
        fmt='%H:%M:%S %Z' if start.utcoffset()!=finish.utcoffset() or start.fold or finish.fold else '%H:%M:%S'
        if start.date()!=finish.date():fmt='%d.%m '+fmt
        time_cells=[('Начало',start.strftime(fmt)),('Конец',finish.strftime(fmt)),('Выбрано по времени' if many else 'Длительность',observed_duration(summary['seconds'],t))] if any(not source['event'] for source in rows) else [('Событие',start.strftime(fmt)),('Дата',start.strftime('%d.%m.%Y'))]
        metrics=[];notes=[]
        if many:
            metrics.append(('С наблюдениями',observed_duration(summary['observed_seconds'],t)))
            if summary['gap_seconds']:metrics.append(('Без наблюдений',observed_duration(summary['gap_seconds'],t)))
            notes.append(t('Промежутки между выбранными участками не включены. События учитываются один раз.'))
        counter_names={'actions_performed':'Действий выполнено','pages_processed':'Страниц обработано','checks':'Проверок сит','flow_passes':'Проходов с начала','offices_seen':'Появлений офисов','booking_screens':'Появлений выбора даты','server_errors':'Ошибок сервера','rate_limited':'Ограничений HTTP 429','expired_sessions':'Истекших сессий','blocked':'Ограничений доступа','browser_resets':'Сбросов браузера','unknown_errors':'Неизвестных ошибок','fatal_errors':'Остановок с ошибкой'}
        metrics.extend((name,str(summary['counts'][key])) for key,name in counter_names.items() if summary['counts'][key])
        if summary['offices']:metrics.append(('Разных офисов',str(len(summary['offices']))))
        if kind=='gap' and not many:notes.append(t('Нет записей о состоянии сайта. Проверки и активное время неизвестны.'))
        if summary['restored_seconds']:notes.append(t('Восстановлено по журналам запуска')+': '+observed_duration(summary['restored_seconds'],t))
        elif any(source.get('restored') for source in rows):notes.append(t('Восстановлено по журналам запуска'))
        if any(source.get('source_overlap') for source in rows):notes.append(t('Перекрывающиеся запуски'))
        if not row['event'] and kind!='gap' and not many and not row.get('confirmed_end'):notes.append(t('Окончание неизвестно'))
        if not many and row.get('kind')=='availability':
            for key,caption in [('office_seen_at','Офисы'),('booking_seen_at','Выбор даты')]:
                if row.get(key):notes.append(t(caption)+': '+stamp(row[key]))
        if not many and kind=='restart' and isinstance(row.get('evidence'),dict):
            previous=row['evidence'].get('previous_stage')
            if previous:notes.append(t(LABELS.get(previous,('Новая сессия',))[0]))
        muted=c['muted']
        html=f'<b>{escape(label)}</b> &nbsp; <span style="color:{muted}">{start:%d.%m.%Y}</span><br>'
        html+='<table width="100%" cellspacing="0" cellpadding="0"><tr>'
        for caption,value in time_cells:html+=f'<td width="33%"><span style="color:{muted}">{escape(t(caption))}</span><br>{escape(value)}</td>'
        html+='</tr></table><hr style="color:'+c['line']+';background-color:'+c['line']+';">'
        if metrics:
            html+='<table width="100%" cellspacing="0" cellpadding="3">'
            for i in range(0,len(metrics),2):
                html+='<tr>'
                for caption,value in metrics[i:i+2]:html+=f'<td style="color:{muted}">{escape(t(caption))}</td><td align="right">{escape(value)}</td><td width="12"></td>'
                html+='</tr>'
            html+='</table>'
        offices=list(dict.fromkeys(summary['offices']+[office for source in rows for office in source.get('offices',[])]))
        if offices and all(source.get('event') for source in rows):
            html+='<br>'+'<br>'.join(escape(office) for office in offices[:3])
            if len(offices)>3:html+='<br>'+escape(t('Ещё офисов: {count}').format(count=len(offices)-3))
        self.detail.setToolTip('\n'.join(offices) if len(offices)>3 else '')
        if notes:html+='<br><span style="color:'+muted+'">'+'<br>'.join(escape(note) for note in notes)+'</span>'
        self.detail.setText(html)
        if hasattr(self.host,'control_icon'):
            icon='copy' if many else 'office' if row.get('availability_kind',row['kind'])=='office' else 'calendar_days' if kind=='available' else {'cod':'clock','gap':'alert','recovery':'restart','fatal':'alert','manual':'user'}.get(kind,'activity')
            self.detail_icon.setPixmap(self.host.control_icon(icon).pixmap(18,18))
