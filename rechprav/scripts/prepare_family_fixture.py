#!/usr/bin/env python3
"""Frozen explicit-form controls; not an automatic declension or ASR dataset."""
import json
from pathlib import Path
from prepare_user_phrase_fixture import record

CASES = [
    ("таб и табы", [], ["negative", "base_and_plural"]),
    ("слот слота слоте", [], ["negative", "correct_forms"]),
    ("ТАБ и СЛОТ", [], ["negative", "uppercase"]),
    ("этабы", [("этабы", "табы")], ["positive", "plural"]),
    ("свуты", [("свуты", "слот")], ["positive", "base"]),
    ("без снятия сваты", [("сваты", "слота")], ["positive", "genitive"]),
    ("ошибка в свате", [("свате", "слоте")], ["positive", "new_form"]),
    ("сваты приехали", [], ["negative", "known_word"]),
    ("думаю о свате", [], ["negative", "known_word"]),
    ("свуты и без снятия сваты", [("свуты", "слот"), ("сваты", "слота")], ["positive", "multiple_forms"]),
    ("этабы и ошибка в свате", [("этабы", "табы"), ("свате", "слоте")], ["positive", "two_families"]),
    ("таб и этабы", [("этабы", "табы")], ["positive", "protect_base"]),
    ("слота и свуты", [("свуты", "слот")], ["positive", "protect_form"]),
    ("слово этабы содержит ошибку", [], ["negative", "literal_alias"]),
    ("слово свуты записано неверно", [], ["negative", "literal_alias"]),
    ("обсуждаем выражение без снятия сваты", [], ["negative", "literal_alias", "matching_context"]),
    ("без снятия\nсваты", [], ["negative", "newline"]),
    ("ошибка в, свате", [], ["negative", "punctuation"]),
    ("`сваты`", [], ["negative", "code"]),
    ("https://host/этабы", [], ["negative", "url"]),
    ("проковол колесо", [("проковол", "проколол")], ["positive", "phrase_control"]),
    ("долгождать", [("долгождать", "долго ждать")], ["positive", "split_control"]),
    ("без снятия\tсваты!", [("сваты", "слота")], ["positive", "whitespace"]),
    ("слоте и таб", [], ["negative", "correct_forms"]),
]

def main():
    out = Path(__file__).resolve().parents[1] / "data/fixtures/term-families/corpus.jsonl"
    out.parent.mkdir(parents=True)
    with out.open("x", encoding="utf-8") as f:
        for i,(source,edits,tags) in enumerate(CASES,1):
            r=record(f"family-{i:03}",source,edits,tags)
            r["provenance"]["origin"]="authored 011 explicit-family controls before implementation; not independent holdout"
            f.write(json.dumps(r,ensure_ascii=False)+"\n")

if __name__ == "__main__": main()
