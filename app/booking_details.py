"""Bounded, passive extraction of public appointment offers, never form values."""
import asyncio
from datetime import date, datetime
import re

MAX_OFFERS = 8
# Known ICP radio layout. The time dropdown is a search filter, not an offer.
OFFER_LABELS = """() => {
    const visible = el => !!el && el.getClientRects().length > 0 &&
        getComputedStyle(el).visibility === 'visible' && getComputedStyle(el).display !== 'none';
    return Array.from(document.querySelectorAll('input[type="radio"][name="rdbCita"]'))
        .filter(el => !el.matches(':disabled') && visible(el))
        .slice(0, 32).map(el => Array.from(el.labels || []).filter(visible)
            .map(label => label.innerText).join(' ').slice(0, 240));
}"""


def parse_offers(labels):
    offers=[]
    if not isinstance(labels,list):return offers
    for label in labels[:32]:
        if not isinstance(label,str):continue
        match=re.search(r'D[ií]a\s*:\s*(\d{2}/\d{2}/\d{4})\s+Hora\s*:\s*(\d{2}:\d{2})(?!\d)', ' '.join(label[:240].split()),re.I)
        if not match:continue
        try:value=datetime.strptime(' '.join(match.groups()),'%d/%m/%Y %H:%M')
        except ValueError:continue
        offer={'date':value.strftime('%Y-%m-%d'),'time':value.strftime('%H:%M')}
        if offer not in offers:offers.append(offer)
        if len(offers)>=MAX_OFFERS:break
    return offers


async def read_booking_details(source, *, office='', timeout=.75):
    try:labels=await asyncio.wait_for(source.evaluate(OFFER_LABELS),timeout=timeout)
    except Exception:return {}
    details={}
    if isinstance(office,str) and office.strip():details['office']=' '.join(office.split())[:320]
    offers=parse_offers(labels)
    if offers:details['offers']=offers
    return details


def normalize_booking_details(raw):
    """Strict public-field contract for messages, journals and durable history."""
    if not isinstance(raw,dict):return {}
    result={}
    office=raw.get('office')
    if isinstance(office,str) and office.strip():result['office']=' '.join(office.split())[:320]
    offers=[]
    source=raw.get('offers',[])
    for row in source[:MAX_OFFERS] if isinstance(source,list) else []:
        if not isinstance(row,dict):continue
        try:day=date.fromisoformat(row.get('date',''))
        except (ValueError,TypeError):continue
        hour=row.get('time','')
        if not isinstance(hour,str) or not re.fullmatch(r'(?:[01]\d|2[0-3]):[0-5]\d',hour):continue
        offer={'date':day.isoformat(),'time':hour}
        if offer not in offers:offers.append(offer)
    if offers:result['offers']=offers
    return result


def booking_lines(raw):
    details=normalize_booking_details(raw)
    lines=[details['office']] if details.get('office') else []
    lines.extend(date.fromisoformat(offer['date']).strftime('%d.%m.%Y')+' — '+offer['time'] for offer in details.get('offers',[]))
    return lines
