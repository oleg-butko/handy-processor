#!/usr/bin/env python3
"""013 authored edit-cost controls, frozen before implementation."""
import json
from pathlib import Path
from prepare_user_phrase_fixture import record
CASES=[
 ('клиппорд',[('клиппорд','клипборд')],['positive','novel','one_pair']),
 ('клиппорт',[('клиппорт','клипборд')],['positive','novel','two_pairs']),
 ('клибпорт',[('клибпорт','клипборд')],['positive','novel','three_pairs']),
 ('клипбар',[('клипбар','клипборд')],['positive','novel','vowel_gap']),
 ('лакалхост',[('лакалхост','локалхост')],['positive','novel','vowel']),
 ('локолхост',[('локолхост','локалхост')],['positive','novel','vowel']),
 ('лакалхасд',[('лакалхасд','локалхост')],['positive','novel','three_pairs']),
 ('локалхос',[('локалхос','локалхост')],['positive','novel','deletion']),
 ('локалхостт',[('локалхостт','локалхост')],['positive','novel','insertion']),
 ('недосвон',[('недосвон','недозвон')],['positive','novel','one_pair']),
 ('нетосвон',[('нетосвон','недозвон')],['positive','novel','two_pairs']),
 ('нидосвон',[('нидосвон','недозвон')],['positive','novel','vowel']),
 ('пээнсэ',[('пээнсэ','пнз')],['positive','novel','pronunciation']),
 ('бээнсэ',[('бээнсэ','пнз')],['positive','novel','two_pairs','pronunciation']),
 ('пэ эн зэ',[('пэ эн зэ','пнз')],['positive','exact_compact','pronunciation']),
 ('слод',[('слод','слот')],['positive','novel','one_pair']),
 ('без снятия слоды',[('слоды','слота')],['positive','novel','form']),
 ('в слоде',[('слоде','слоте')],['positive','novel','form']),
 ('этабы',[('этабы','табы')],['positive','novel','insertion']),
 ('долгождать',[('долгождать','долго ждать')],['positive','exact_compact','phrase']),
 ('луколхоз',[('луколхоз','локалхост')],['positive','far_error']),
 ('свуты',[('свуты','слот')],['positive','far_error']),
 ('слот слота слоте',[],['negative','canonical']),
 ('таб и табы',[],['negative','canonical']),
 ('свод правил',[],['negative','known']),
 ('ввод данных',[],['negative','known']),
 ('плод созрел',[],['negative','known']),
 ('сладкий солод',[],['negative','known']),
 ('снять шторы',[],['negative','known']),
 ('без снятия сваты',[],['negative','ambiguous_context']),
 ('в свате',[],['negative','known']),
 ('слово клиппорт написано неверно',[],['negative','literal','oov']),
 ('слово нетосвон придумано',[],['negative','literal','oov']),
 ('упоминаем бээнсэ как ошибку',[],['negative','literal','oov']),
 ('`клиппорт`',[],['negative','code']),
 ('https://host/клиппорт',[],['negative','url']),
 ('КЛИППОРТ',[],['negative','uppercase']),
 ('пэ, эн зэ',[],['negative','punctuation']),
 ('пэ\nэн зэ',[],['negative','newline']),
 ('табы и клиппорт',[('клиппорт','клипборд')],['positive','mixed']),
]
def main():
 out=Path(__file__).resolve().parents[1]/'data/fixtures/weighted'
 out.mkdir(parents=True)
 with (out/'corpus.jsonl').open('x') as f:
  for i,(source,edits,tags) in enumerate(CASES,1):
   r=record(f'weighted-{i:03}',source,edits,tags)
   r['provenance']['origin']='013 authored cost-hypothesis dev controls; no independent ASR claim'
   f.write(json.dumps(r,ensure_ascii=False)+'\n')
if __name__=='__main__':main()
