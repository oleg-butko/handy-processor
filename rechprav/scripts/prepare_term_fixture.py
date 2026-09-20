#!/usr/bin/env python3
"""Author a fixed synthetic span corpus; no ASR history or automatic gold guesses."""
import json
from pathlib import Path

# Exact user examples are listed aliases; novel variants test generalization mechanics only.
CASES = [
    ("проверь пээнзэ", [("пээнзэ", "ПНЗ")], ["listed", "pnz"]),
    ("проверь пэнзэ", [("пэнзэ", "ПНЗ")], ["listed", "pnz"]),
    ("проверь пензе", [("пензе", "ПНЗ")], ["listed", "pnz", "real_word"]),
    ("проверь пензэ", [("пензэ", "ПНЗ")], ["listed", "pnz"]),
    ("проверь бензэ", [("бензэ", "ПНЗ")], ["listed", "pnz"]),
    ("проверь пэ эн зэ", [("пэ эн зэ", "ПНЗ")], ["listed", "pnz", "multiword"]),
    ("проверь пэн зэ", [("пэн зэ", "ПНЗ")], ["listed", "pnz", "multiword"]),
    ("проверь бэнзэ", [("бэнзэ", "ПНЗ")], ["novel", "pnz"]),
    ("проверь пэнзе", [("пэнзе", "ПНЗ")], ["novel", "pnz"]),
    ("проверь бэнзе", [("бэнзе", "ПНЗ")], ["novel", "pnz", "distance_two"]),
    ("проверь пэ эн зе", [("пэ эн зе", "ПНЗ")], ["novel", "pnz", "multiword"]),
    ("проверь пэ энзэ", [("пэ энзэ", "ПНЗ")], ["novel", "pnz", "multiword"]),
    ("проверь бэн зэ", [("бэн зэ", "ПНЗ")], ["novel", "pnz", "multiword"]),
    ("вставь модуль в свод памяти", [("свод", "слот")], ["listed", "slot", "real_word"]),
    ("проверь ввод памяти", [("ввод", "слот")], ["listed", "slot", "real_word"]),
    ("вставь плату в свод", [("свод", "слот")], ["listed", "slot", "missing_cue"]),
    ("пэнзэ", [("пэнзэ", "ПНЗ")], ["listed", "pnz", "no_context"]),
    ("пензе", [("пензе", "ПНЗ")], ["listed", "pnz", "no_context", "real_word"]),
    ("пэнзэ на стенде", [("пэнзэ", "БНЗ")], ["listed", "lab_intent", "conflict"]),
    ("сигнал бензэ в лаборатории", [("бензэ", "БНЗ")], ["listed", "lab_intent", "conflict"]),
    ("проверь пэ эн зэ и сигнал пэ эн зэ", [("пэ эн зэ", "ПНЗ"), ("пэ эн зэ", "ПНЗ")], ["listed", "pnz", "multiword", "multiple_spans"]),
    ("проверь\tпэ\tэн  зэ!", [("пэ\tэн  зэ", "ПНЗ")], ["listed", "pnz", "multiword", "whitespace"]),
    ("я живу в пензе", [], ["negative", "city", "real_word"]),
    ("в Пензе сегодня тепло", [], ["negative", "city", "titlecase"]),
    ("в пензе проверь сигнал", [], ["negative", "city", "misleading_context"]),
    ("модуль обрабатывает ввод данных", [], ["negative", "slot", "misleading_context"]),
    ("плата содержит свод правил", [], ["negative", "slot", "misleading_context"]),
    ("ввод данных завершён", [], ["negative", "slot", "real_word"]),
    ("изучи свод правил", [], ["negative", "slot", "real_word"]),
    ("проверь ПНЗ и слот памяти", [], ["negative", "canonical"]),
    ("проверь БНЗ", [], ["negative", "canonical"]),
    ("проверь `пэнзэ`", [], ["negative", "code"]),
    ("проверь https://example.org/пэнзэ", [], ["negative", "url"]),
    ("проверь ./пэнзэ.txt", [], ["negative", "path"]),
    ("проверь пэнзэ.txt", [], ["negative", "filename"]),
    ("проверь ПЭНЗЭ", [], ["negative", "uppercase"]),
    ("проверь пэ, эн зэ", [], ["negative", "punctuation"]),
    ("проверь пэ эн\nзэ", [], ["negative", "newline"]),
    ("проверь пенсия", [], ["negative", "similar_word"]),
    ("проверь бензин", [], ["negative", "similar_word"]),
    ("проверь пемза", [], ["negative", "similar_word"]),
    ("проверь пение", [], ["negative", "similar_word"]),
    ("обычная запись без терминов", [], ["negative", "control"]),
]

def main():
    root = Path(__file__).resolve().parents[1]
    output = root / "data/fixtures/terms/corpus.jsonl"
    output.parent.mkdir(exist_ok=True)
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
            record = {"schema_version": 1, "id": f"terms-{index:03}", "group_id": f"terms-{index:03}", "split": "dev",
                      "source": source, "gold": "".join(parts), "edits": edits, "tags": tags,
                      "provenance": {"kind": "synthetic", "origin": "manually authored term stress cases; includes user-suggested pronunciations, not real ASR", "asr_model": None},
                      "annotation_status": "reviewed"}
            stream.write(json.dumps(record, ensure_ascii=False) + "\n")
    print(output)

if __name__ == "__main__":
    main()
