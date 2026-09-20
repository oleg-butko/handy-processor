#!/usr/bin/env python3
"""Authored context stress cases, frozen before experiment 007; not real ASR."""
import json
from pathlib import Path

CASES = [
    ("проверь пензе модуля", [("пензе", "ПНЗ")], ["positive", "known", "pnz"]),
    ("сигнал пэнзэ модуля пропал", [("пэнзэ", "ПНЗ")], ["positive", "pnz"]),
    ("сигнал бэнзэ модуля пропал", [("бэнзэ", "ПНЗ")], ["positive", "novel", "pnz"]),
    ("проверь пение модуля", [("пение", "ПНЗ")], ["positive", "known_fuzzy", "pnz"]),
    ("сигнал пэ эн зе пропал", [("пэ эн зе", "ПНЗ")], ["positive", "multiword", "novel"]),
    ("проверь пэ эн зэ модуля", [("пэ эн зэ", "ПНЗ")], ["positive", "multiword"]),
    ("сигнал пэ энзэ пропал", [("пэ энзэ", "ПНЗ")], ["positive", "multiword", "novel"]),
    ("активируй бэнзэ", [("бэнзэ", "ПНЗ")], ["positive", "missing_context", "novel"]),
    ("пожалуйста включи пэнзэ", [("пэнзэ", "ПНЗ")], ["positive", "missing_context"]),
    ("вставь плату в ввод", [("ввод", "слот")], ["positive", "known", "slot"]),
    ("осмотри свод для платы", [("свод", "слот")], ["positive", "known", "slot"]),
    ("ввод памяти повреждён", [("ввод", "слот")], ["positive", "known", "slot"]),
    ("вставь новую плату в свод", [("свод", "слот")], ["positive", "pattern_gap", "slot"]),
    ("проверь исправный пензе модуля", [("пензе", "ПНЗ")], ["positive", "known", "right_context"]),
    ("модули пензе неисправны", [("пензе", "ПНЗ")], ["positive", "missing_inflection"]),
    ("включи пэнзэ на стенде", [("пэнзэ", "БНЗ")], ["positive", "lab", "routing"]),
    ("наблюдай бензэ в лаборатории", [("бензэ", "БНЗ")], ["positive", "lab", "routing"]),
    ("датчик бэнзэ неисправен", [("бэнзэ", "БНЗ")], ["positive", "lab", "routing", "novel"]),
    ("проверь пэнзэ на стенде", [("пэнзэ", "БНЗ")], ["positive", "lab", "conflicting_context"]),
    ("сигнал пензе в лаборатории", [("пензе", "БНЗ")], ["positive", "lab", "conflicting_context", "known"]),
    ("проверь пэнзэ модуля и датчик бензэ", [("пэнзэ", "ПНЗ"), ("бензэ", "БНЗ")], ["positive", "routing", "multiple_spans"]),
    ("проверь пензе на карте", [], ["negative", "city", "exclusion"]),
    ("модуль пензе на карте отмечен", [], ["negative", "city", "exclusion"]),
    ("сигнал проверь когда будешь в пензе", [], ["negative", "city", "distant_cue"]),
    ("город пензе проверь завтра", [], ["negative", "city", "exclusion"]),
    ("проверь ввод данных", [], ["negative", "slot", "exclusion"]),
    ("проверь свод правил", [], ["negative", "slot", "exclusion"]),
    ("проверь пение хора", [], ["negative", "known_fuzzy"]),
    ("процессор ускоряет ввод памяти", [], ["negative", "slot", "unmodeled_semantics"]),
    ("проверь свод законодательных правил", [], ["negative", "slot", "exclusion_gap"]),
    ("модуль показывает пэнзэ на карте", [], ["negative", "oov_exclusion"]),
    ("проверь пэ эн зэ на карте", [], ["negative", "multiword", "exclusion"]),
    ("проверь пензе. на карте", [], ["negative", "punctuation", "exclusion_boundary"]),
    ("проверь пензе\nна карте", [], ["negative", "newline", "exclusion_boundary"]),
    ("проверь. пензе на карте", [], ["negative", "punctuation", "positive_boundary"]),
    ("проверь\nпензе на карте", [], ["negative", "newline", "positive_boundary"]),
    ("проверь `пэнзэ` модуля", [], ["negative", "code"]),
    ("сигнал https://host/пэнзэ модуля", [], ["negative", "url"]),
    ("датчик ПНЗ и БНЗ исправен", [], ["negative", "canonical"]),
    ("проверь ПЭНЗЭ модуля", [], ["negative", "uppercase"]),
    ("проверь\tпензе\tна  карте", [], ["negative", "exclusion", "whitespace"]),
    ("проверь пэ эн\nзэ модуля", [], ["negative", "partial_pronunciation"]),
]

def main():
    output = Path(__file__).resolve().parents[1] / "data/fixtures/term-context/corpus.jsonl"
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
            record = {"schema_version": 1, "id": f"context-{index:03}", "group_id": f"context-{index:03}", "split": "dev",
                      "source": source, "gold": "".join(parts), "edits": edits, "tags": tags,
                      "provenance": {"kind": "synthetic", "origin": "authored context contrast cases before 007 implementation; not independent ASR holdout", "asr_model": None},
                      "annotation_status": "reviewed"}
            stream.write(json.dumps(record, ensure_ascii=False) + "\n")
    print(output)

if __name__ == "__main__":
    main()
