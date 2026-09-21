#!/usr/bin/env python3
"""012: authored feedback controls, including ambiguous ordinary-language contexts."""
import json
from pathlib import Path
from prepare_user_phrase_fixture import record
CASES = [
 ('сняли свод', [('свод','слот')], ['positive','seen_event']),
 ('сняли свод памяти', [('свод','слот')], ['positive','seen_context']),
 ('мы сняли свод вчера', [('свод','слот')], ['positive','seen_context']),
 ('сняли ввод', [('ввод','слот')], ['positive','other_alias']),
 ('освободили свод памяти', [('свод','слот')], ['positive','unseen_context']),
 ('проверь свод памяти', [('свод','слот')], ['positive','unseen_context']),
 ('свод правил', [], ['negative','known_word']),
 ('ввод данных', [], ['negative','known_word']),
 ('свод законов', [], ['negative','known_word']),
 ('каменный свод', [], ['negative','architecture']),
 ('сняли свод старой арки', [], ['negative','matching_context','architecture']),
 ('рабочие сняли свод здания', [], ['negative','matching_context','architecture']),
 ('обсуждаем выражение сняли свод', [], ['negative','literal','matching_context']),
 ('слово свод написано правильно', [], ['negative','literal']),
 ('сняли\nсвод', [], ['negative','newline']),
 ('сняли, свод', [], ['negative','punctuation']),
 ('сняли\tсвод!', [('свод','слот')], ['positive','whitespace']),
 ('Сняли свод', [('свод','слот')], ['positive','case']),
 ('сняли СВОД', [], ['negative','uppercase']),
 ('сняли `свод`', [], ['negative','code']),
 ('https://host/свод', [], ['negative','url']),
 ('сняли слот', [], ['negative','correct_target']),
 ('слот и свод правил', [], ['negative','correct_target','known_word']),
 ('сняли своты', [('своты','слот')], ['positive','no_candidate']),
 ('нет звон', [('нет звон','недозвон')], ['positive','multiword']),
 ('обсуждаем выражение нет звон', [], ['negative','multiword','literal']),
 ('нет, звон', [], ['negative','punctuation','multiword']),
 ('нет\nзвон', [], ['negative','newline','multiword']),
 ('сняли ввод и свод правил', [('ввод','слот')], ['positive','mixed']),
 ('открой клипборд', [], ['negative','unrelated']),
]
def main():
 out=Path(__file__).resolve().parents[1]/'data/fixtures/feedback'
 out.mkdir(parents=True)
 with (out/'corpus.jsonl').open('x') as f:
  for i,(source,edits,tags) in enumerate(CASES,1):
   r=record(f'feedback-{i:03}',source,edits,tags)
   r['provenance']['origin']='authored 012 synthetic/dev controls; example edit is illustrative, not real ASR'
   f.write(json.dumps(r,ensure_ascii=False)+'\n')
 with (out/'example-event.json').open('x') as f:
  json.dump({'schema_version':1,'event_id':'illustrative-001','origin':'synthetic illustration from user idea, not observed editor event','confirmed_asr_correction':True,'before':'сняли свод','after':'сняли слот','source_span':{'byte_start':11,'byte_end':19,'text':'свод'},'replacement':'слот','left_context':['сняли'],'right_context':[],'target_set':'technical','status':'illustrative_only','retracted_by':None},f,ensure_ascii=False,indent=2)
  f.write('\n')
if __name__=='__main__': main()
