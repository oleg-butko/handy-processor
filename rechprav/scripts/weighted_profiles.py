"""013 fixed cost ablations; all aliases, weights and known gates shared."""
import copy
from family_profiles import profiles as previous

def profiles(root):
 c=previous(root)['family']
 for t in c['sets'][1]['terms']:
  t['aliases']=[t['canonical']] if t['id']!='pnz' else ['пээнзэ']
  for f in t.get('forms',[]):f['aliases']=[f['canonical']]
 def pairs(groups):return [{'from':a,'to':b,'cost':50} for x,y in groups for a,b in [(x,y),(y,x)]]
 voiced=pairs(['бп','вф','гк','дт','жш','зс']); vowels=voiced+pairs(['ао','еи'])
 result={}
 for name in ['edit-1','edit-2','weighted-uniform','weighted-pairs','weighted-vowels','weighted-wide','weighted-gaps']:
  x=copy.deepcopy(c)
  if name=='edit-2':x['policy']['max_distance']=2
  if name.startswith('weighted'):
   x['policy']['weighted_distance']={'max_cost':150 if name=='weighted-wide' else 100,
    'insertion':75 if name=='weighted-gaps' else 100,'deletion':75 if name=='weighted-gaps' else 100,
    'substitutions':[] if name=='weighted-uniform' else (voiced if name=='weighted-pairs' else vowels)}
  result[name]=x
 return result
