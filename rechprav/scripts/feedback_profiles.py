"""Frozen 012 source/target/context ablations; no fitting after measurement."""
import copy
from family_profiles import profiles as families

def profiles(root):
 old=families(root)['family']
 c={'baseline_config':old['baseline_config'],'enabled':True,'policy':copy.deepcopy(old['policy']),
    'sets':[{'id':'technical','enabled':True,'boost':0.0,'terms':[
     {'id':'slot','canonical':'слот','aliases':['слот','свод','ввод'],'enabled':True,'boost':-2.0,'known_words':'always','max_known_distance':0,'context':[]},
     {'id':'noanswer','canonical':'недозвон','aliases':['недозвон','нет звон'],'enabled':True,'boost':-2.0,'known_words':'always','max_known_distance':0,'context':[]}]}]}
 result={}
 names=['baseline','target-only','source-only','joint-global','joint-context','hybrid','source-off','target-off','duplicate','conflict','preferred','pair-context','strict-known']
 for name in names:
  x=copy.deepcopy(c)
  if name not in ['baseline','source-only','pair-context']:
   for t in x['sets'][0]['terms']:t['boost']+=1
  if name not in ['baseline','target-only','pair-context']:
   rules=[]
   contextual=name not in ['source-only','joint-global']
   for i,source in enumerate(['свод','ввод','нет звон']):
    r={'id':f'source{i}','enabled':name!='source-off','source':source,'penalty':2.0 if name=='source-only' else 1.0}
    if contextual:r['context_patterns']=['сняли {term}']
    rules.append(r)
    if name=='hybrid':rules.append({'id':f'weak{i}','enabled':True,'source':source,'penalty':0.5})
   if name=='duplicate':
    r=copy.deepcopy(rules[0]);r['id']='duplicate';rules.append(r)
   x['source_penalties']=rules
  if name=='target-off':x['sets'][0]['enabled']=False
  if name=='pair-context':x['sets'][0]['terms'][0]['context_patterns']=['сняли {term}']
  if name in ['conflict','preferred','pair-context']:
   rival=copy.deepcopy(x['sets'][0]);rival['id']='competing';rival['terms']=rival['terms'][:1]
   rival['terms'][0].update(id='switch',canonical='свитч',aliases=['свод','ввод'])
   rival['terms'][0].pop('context_patterns',None)
   x['sets'].append(rival)
  if name=='preferred':x['sets'][0]['boost']=1.0
  if name=='strict-known':
   for t in x['sets'][0]['terms']:t['known_words']='never'
  result[name]=x
 return result
