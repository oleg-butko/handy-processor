#!/usr/bin/env python3
"""014 fixed editor events and separate authored correction controls."""
import json
from pathlib import Path
from prepare_user_phrase_fixture import record

def correction(id,before,after,scope='technical',confirmed=True,**extra):
 return dict(schema_version=1,event_id=id,kind='correction',scope=scope,before=before,after=after,confirmed_asr=confirmed,**extra)
def retract(id,target):return dict(schema_version=1,event_id=id,kind='retract',target_event_id=target)
EVENTS=[
 correction('slot','сняли свод памяти','сняли слот памяти'),
 correction('clipboard','открой калиборд','открой клипборд'),
 correction('split','надо долгождать ответа','надо долго ждать ответа'),
 correction('term','включи пензэ','включи ПНЗ'),
 correction('word','он проковол колесо','он проколол колесо'),
 correction('form','без снятия сваты','без снятия слота'),
 correction('isolated','Нет звон','недозвон'),
 correction('style','очень хороший день','прекрасный день',confirmed=False),
 correction('two-edits','свод и ввод','слот и вход'),
 correction('format','свод  памяти','слот памяти'),
 correction('delete','таб таб','таб'),
 correction('case-only','слот','Слот'),
 correction('code','`свод`','`слот`'),
 correction('other','сняли свод памяти','сняли свитч памяти',scope='other'),
]
CASES=[
 ('сняли свод памяти',[('свод','слот')],['positive','seen_event']),
 ('вчера сняли свод памяти',[('свод','слот')],['positive','seen_context']),
 ('сняли свод',[('свод','слот')],['positive','unseen_context']),
 ('освободили свод памяти',[('свод','слот')],['positive','unseen_context']),
 ('свод правил',[],['negative','ordinary']),
 ('ввод данных',[],['negative','ordinary']),
 ('сняли свод старой арки',[],['negative','architecture']),
 ('обсуждаем выражение сняли свод памяти',[],['negative','literal','matching_context']),
 ('открой калиборд',[('калиборд','клипборд')],['positive','seen_context']),
 ('покажи калиборд',[('калиборд','клипборд')],['positive','unseen_context']),
 ('надо долгождать ответа',[('долгождать','долго ждать')],['positive','split']),
 ('долгождать',[('долгождать','долго ждать')],['positive','unseen_context']),
 ('включи пензэ',[('пензэ','ПНЗ')],['positive','new_term']),
 ('пензэ',[('пензэ','ПНЗ')],['positive','unseen_context']),
 ('он проковол колесо',[('проковол','проколол')],['positive','dictionary_word']),
 ('без снятия сваты',[('сваты','слота')],['positive','existing_form']),
 ('сваты приехали',[],['negative','ordinary']),
 ('Нет звон',[('Нет звон','недозвон')],['positive','no_context','known_source']),
 ('обсуждаем выражение нет звон',[],['negative','literal','known_source']),
 ('сняли\nсвод памяти',[],['negative','newline']),
 ('сняли, свод памяти',[],['negative','punctuation']),
 ('сняли `свод` памяти',[],['negative','code']),
 ('https://host/калиборд',[],['negative','url']),
 ('Сняли свод памяти!',[('свод','слот')],['positive','case']),
 ('слот слота таб табы',[],['negative','canonical']),
 ('слово пензэ написано ошибочно',[],['negative','literal']),
]
def main():
 out=Path(__file__).resolve().parents[1]/'data/fixtures/feedback-drafts';out.mkdir(parents=True)
 for name,rows in [('events.jsonl',EVENTS),('corpus.jsonl',[record(f'draft-{i:03}',s,e,t) for i,(s,e,t) in enumerate(CASES,1)])]:
  with (out/name).open('x') as f:
   for r in rows:
    if 'provenance' in r:r['provenance']['origin']='014 authored dev controls; editor events are synthetic illustrations'
    f.write(json.dumps(r,ensure_ascii=False)+'\n')
if __name__=='__main__':main()
