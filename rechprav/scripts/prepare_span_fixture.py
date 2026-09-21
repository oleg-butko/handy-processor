#!/usr/bin/env python3
"""Authored span boundary cases frozen before 008; not observed ASR data."""
import json
from pathlib import Path

CASES = [
    ("проверь пэ энзэ", [("пэ энзэ", "ПНЗ")], ["whole", "known_syllable"]),
    ("сигнал пэ энзэ модуля", [("пэ энзэ", "ПНЗ")], ["whole", "both_contexts"]),
    ("проверь пэ эн зэ", [("пэ эн зэ", "ПНЗ")], ["whole", "three_tokens"]),
    ("проверь пэн зэ", [("пэн зэ", "ПНЗ")], ["whole", "two_tokens"]),
    ("проверь пэ эн зе", [("пэ эн зе", "ПНЗ")], ["whole", "known_fuzzy"]),
    ("сигнал пээн зэ модуля", [("пээн зэ", "ПНЗ")], ["whole", "alternate_split"]),
    ("проверь п энзэ", [("п энзэ", "ПНЗ")], ["whole", "single_letter"]),
    ("проверь пэ  энзэ!", [("пэ  энзэ", "ПНЗ")], ["whole", "whitespace"]),
    ("Проверь\tпэ\tэнзэ!", [("пэ\tэнзэ", "ПНЗ")], ["whole", "tab"]),
    ("сигнал пэ\u00a0энзэ модуля", [("пэ\u00a0энзэ", "ПНЗ")], ["whole", "nbsp"]),
    ("пэ энзэ модуля", [("пэ энзэ", "ПНЗ")], ["whole", "right_context"]),
    ("включи пэ энзэ", [("пэ энзэ", "ПНЗ")], ["whole", "missing_context"]),
    ("датчик пэ энзэ", [("пэ энзэ", "БНЗ")], ["whole", "lab"]),
    ("пэ энзэ на стенде", [("пэ энзэ", "БНЗ")], ["whole", "lab", "right_context"]),
    ("проверь пэ энзэ на стенде", [("пэ энзэ", "БНЗ")], ["whole", "conflicting_context"]),
    ("проверь пэ энзэ и сигнал пэнзэ", [("пэ энзэ", "ПНЗ"), ("пэнзэ", "ПНЗ")], ["whole", "two_terms"]),
    ("проверь пэнзэ пэ энзэ", [("пэнзэ", "ПНЗ"), ("пэ энзэ", "ПНЗ")], ["whole", "adjacent_terms"]),
    ("сигнал пэ энзэ модуля и датчик пэ энзэ", [("пэ энзэ", "ПНЗ"), ("пэ энзэ", "БНЗ")], ["whole", "routing", "two_terms"]),
    ("обозначь символом пэ энзэ модуля", [("энзэ", "ПНЗ")], ["fragment", "correct_letter_neighbor"]),
    ("обозначь буквой пэ энзэ модуля", [("энзэ", "ПНЗ")], ["fragment", "correct_letter_neighbor"]),
    ("проверь символ пэ энзэ модуля", [("энзэ", "ПНЗ")], ["fragment", "correct_letter_neighbor"]),
    ("обозначь символом пэ энзэ на стенде", [("энзэ", "БНЗ")], ["fragment", "correct_letter_neighbor", "lab"]),
    ("проверь я пэнзэ", [("пэнзэ", "ПНЗ")], ["fragment", "correct_word_neighbor"]),
    ("проверь пэнзэ и ввод данных", [("пэнзэ", "ПНЗ")], ["fragment", "correct_word_neighbor", "exclusion"]),
    ("проверь в пэнзэ модуля", [("пэнзэ", "ПНЗ")], ["fragment", "correct_word_neighbor"]),
    ("проверь пэнзэ а", [("пэнзэ", "ПНЗ")], ["fragment", "correct_word_neighbor"]),
    ("проверь энзэ модуля", [("энзэ", "ПНЗ")], ["fragment", "standalone"]),
    ("проверь пэ энзэ на карте", [], ["negative", "whole", "exclusion"]),
    ("живу в пэ энзэ", [], ["negative", "whole", "exclusion"]),
    ("проверь пэ\nэнзэ на карте", [], ["negative", "newline"]),
    ("проверь пэ, энзэ на карте", [], ["negative", "punctuation"]),
    ("проверь пэ эн\r\nзэ на карте", [], ["negative", "crlf"]),
    ("проверь `пэ энзэ`", [], ["negative", "code"]),
    ("сигнал https://host/пэ/энзэ", [], ["negative", "url"]),
    ("проверь ПЭ ЭНЗЭ", [], ["negative", "uppercase"]),
    ("проверь ПНЗ и БНЗ", [], ["negative", "canonical"]),
    ("проверь ввод данных", [], ["negative", "known_word"]),
    ("проверь свод правил", [], ["negative", "known_word"]),
    ("проверь пэ отдельно", [], ["negative", "literal_letter"]),
    ("проверь эн и зэ отдельно", [], ["negative", "literal_letters"]),
]


def main():
    output = Path(__file__).resolve().parents[1] / "data/fixtures/term-spans/corpus.jsonl"
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
            record = {"schema_version": 1, "id": f"span-{index:03}", "group_id": f"span-{index:03}", "split": "dev",
                      "source": source, "gold": "".join(parts), "edits": edits, "tags": tags,
                      "provenance": {"kind": "synthetic", "origin": "authored span contrast cases before 008 implementation; not independent ASR holdout", "asr_model": None},
                      "annotation_status": "reviewed"}
            stream.write(json.dumps(record, ensure_ascii=False) + "\n")
    print(output)


if __name__ == "__main__":
    main()
