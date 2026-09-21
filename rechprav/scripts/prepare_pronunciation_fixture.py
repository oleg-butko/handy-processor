#!/usr/bin/env python3
"""Fixed letter-pronunciation examples, authored before 009, not real ASR."""
import json
from pathlib import Path

CASES = [
    ("проверь пээнзэ", [("пээнзэ", "ПНЗ")], ["pnz", "joined"]),
    ("проверь пэ энзэ", [("пэ энзэ", "ПНЗ")], ["pnz", "two_tokens"]),
    ("проверь пээн зэ", [("пээн зэ", "ПНЗ")], ["pnz", "two_tokens"]),
    ("проверь пэ эн зэ", [("пэ эн зэ", "ПНЗ")], ["pnz", "three_tokens"]),
    ("проверь пэнзэ", [("пэнзэ", "ПНЗ")], ["pnz", "observed_alias"]),
    ("проверь бензэ", [("бензэ", "ПНЗ")], ["pnz", "observed_alias"]),
    ("проверь пензе", [("пензе", "ПНЗ")], ["pnz", "known_alias"]),
    ("датчик бэ эн зэ", [("бэ эн зэ", "БНЗ")], ["bnz", "three_tokens"]),
    ("бээнзэ на стенде", [("бээнзэ", "БНЗ")], ["bnz", "joined"]),
    ("проверь бэ эн зэ на стенде", [("бэ эн зэ", "БНЗ")], ["bnz", "conflicting_context"]),
    ("контроллер пэ эль ка", [("пэ эль ка", "ПЛК")], ["plk", "three_tokens"]),
    ("проверь пээлька", [("пээлька", "ПЛК")], ["plk", "joined"]),
    ("проверь пэ элка", [("пэ элка", "ПЛК")], ["plk", "alternate_name"]),
    ("проверь пэлка", [("пэлка", "ПЛК")], ["plk", "manual_alias"]),
    ("проверь а цэ пэ", [("а цэ пэ", "АЦП")], ["acp", "three_tokens", "known_syllable"]),
    ("проверь ацэпэ", [("ацэпэ", "АЦП")], ["acp", "joined"]),
    ("а цэпэ модуля", [("а цэпэ", "АЦП")], ["acp", "two_tokens"]),
    ("проверь о зэ у", [("о зэ у", "ОЗУ")], ["ozu", "three_tokens", "known_syllable"]),
    ("проверь озэу", [("озэу", "ОЗУ")], ["ozu", "joined"]),
    ("проверь озу", [("озу", "ОЗУ")], ["ozu", "canonical_lowercase"]),
    ("проверь эс ка эс", [("эс ка эс", "СКС")], ["sks", "three_tokens"]),
    ("проверь эскаэс", [("эскаэс", "СКС")], ["sks", "joined"]),
    ("проверь эска эс", [("эска эс", "СКС")], ["sks", "two_tokens"]),
    ("проверь пэ\tэль  ка!", [("пэ\tэль  ка", "ПЛК")], ["plk", "whitespace"]),
    ("контроллер пээлька и ацэпэ модуля", [("пээлька", "ПЛК"), ("ацэпэ", "АЦП")], ["multiple_terms"]),
    ("включи пээлька", [("пээлька", "ПЛК")], ["missing_context"]),
    ("проверь пензе на карте", [], ["negative", "city"]),
    ("проверь ввод данных", [], ["negative", "slot"]),
    ("проверь свод правил", [], ["negative", "slot"]),
    ("проверь пение хора", [], ["negative", "known_fuzzy"]),
    ("проверь осу", [], ["negative", "known_fuzzy"]),
    ("проверь эску", [], ["negative", "known_fuzzy"]),
    ("обозначь буквой а цэпэ модуля", [("цэпэ", "АЦП")], ["correct_letter_neighbor"]),
    ("обозначь символом пэ энзэ модуля", [("энзэ", "ПНЗ")], ["correct_letter_neighbor"]),
    ("проверь `пэ эль ка`", [], ["negative", "code"]),
    ("проверь https://host/ацэпэ", [], ["negative", "url"]),
    ("проверь ПНЗ ПЛК АЦП ОЗУ СКС", [], ["negative", "canonical"]),
    ("проверь ПЭ ЭЛЬ КА", [], ["negative", "uppercase"]),
    ("проверь пэ эль\nка", [], ["negative", "newline"]),
    ("проверь а, цэ пэ", [], ["negative", "punctuation"]),
]


def main():
    output = Path(__file__).resolve().parents[1] / "data/fixtures/pronunciation/corpus.jsonl"
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("x", encoding="utf-8") as stream:
        for index, (source, replacements, tags) in enumerate(CASES, 1):
            edits = []; cursor = 0; parts = []
            for original, replacement in replacements:
                start = source.index(original, cursor); end = start + len(original)
                token_start = len(source[:start].split())
                edits.append({"byte_start": len(source[:start].encode()), "byte_end": len(source[:end].encode()),
                              "token_start": token_start, "token_end": token_start + len(original.split()), "replacement": replacement})
                parts.extend([source[cursor:start], replacement]); cursor = end
            parts.append(source[cursor:])
            record = {"schema_version": 1, "id": f"pronunciation-{index:03}", "group_id": f"pronunciation-{index:03}", "split": "dev",
                      "source": source, "gold": "".join(parts), "edits": edits, "tags": tags,
                      "provenance": {"kind": "synthetic", "origin": "authored letter-pronunciation cases before 009 implementation; not real ASR holdout", "asr_model": None},
                      "annotation_status": "reviewed"}
            stream.write(json.dumps(record, ensure_ascii=False) + "\n")
    print(output)


if __name__ == "__main__":
    main()
