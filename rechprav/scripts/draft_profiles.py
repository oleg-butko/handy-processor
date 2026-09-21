"""014 frozen event scenarios; no score tuning from benchmark results."""
import copy
from prepare_draft_fixture import EVENTS,retract

def scenarios():
 result={}
 for name in ['disabled','accepted','no-context','duplicate','repeated','undo-one','undo-last','conflict','other-scope']:
  events=copy.deepcopy(EVENTS);scope='other' if name=='other-scope' else 'technical'
  if name=='duplicate':events.append(copy.deepcopy(events[0]))
  if name in ['repeated','undo-one','undo-last']:
   e=copy.deepcopy(events[0]);e['event_id']='repeat-slot';events.append(e)
  if name in ['undo-one','undo-last']:events.append(retract('undo-slot','slot'))
  if name=='undo-last':events.append(retract('undo-repeat','repeat-slot'))
  if name=='conflict':
   e=copy.deepcopy(events[0]);e.update(event_id='rival',after='сняли свитч памяти');events.append(e)
  result[name]=(events,scope)
 return result
