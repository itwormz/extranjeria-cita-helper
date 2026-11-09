"""Real Python page-priority logic with synthetic, already-visible DOM facts."""
import asyncio
from types import SimpleNamespace
from unittest.mock import AsyncMock, patch
from app.assist_flow import AssistRunner
from app.assist_control import AssistControl


async def recognize(snapshot):
    class Frame:
        def __init__(self, url):
            self.url=url
            self.evaluate=AsyncMock(return_value=snapshot.get('shape',dict(forms=0,selects=0,buttons=0,inputs=[0]*7)))
    host='icp.administracionelectronica.gob.es' if snapshot.get('official',True) else 'auth.example.test'
    url='https://'+host+'/icpplus/'+snapshot.get('endpoint','acCitar')
    frames=[Frame(url+('' if index==0 else '?frame='+str(index))) for index,_ in enumerate(snapshot['frames'])]
    def locator(selector):
        field=SimpleNamespace(is_editable=AsyncMock(return_value=snapshot.get('applicant_editable',True)),
            get_attribute=AsyncMock(return_value=snapshot.get('button_label','')),inner_text=AsyncMock(return_value=snapshot.get('button_label','')))
        field.first=field
        return field
    page=SimpleNamespace(url=url,main_frame=frames[0],locator=locator)
    runner=AssistRunner(page,None,SimpleNamespace(),SimpleNamespace(id='fixture',applicant_fill_mode=snapshot.get('fill_mode','manual')), 'fixture',AssistControl(),None,None)
    runner.last_action=snapshot.get('last_action')
    runner.pending_action=snapshot.get('pending_action')
    runner.visible=AsyncMock(side_effect=lambda selector,source=None:selector in snapshot['visible'])
    runner.http_statuses={frame:(frame.url,record.get('status',200)) for frame,record in zip(frames,snapshot['frames']) if record.get('response_matches',True)}
    docs=[(frame,record['text']) for frame,record in zip(frames,snapshot['frames'])]
    with patch('app.assist_flow._document_key',AsyncMock(return_value='fixture-document')),patch('app.assist_flow.visible_documents',AsyncMock(return_value=docs)):
        result=await runner._observe()
    return dict(kind=result.kind,operation_code=result.operation_code,
        certificate_validation_error=result.certificate_validation_error,recovery_action_first=result.recovery_action_first,
        source_frame=frames.index(result.source),variant=result.variant)


def reference(snapshot):return asyncio.run(recognize(snapshot))
