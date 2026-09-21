#!/usr/bin/env python3
"""User-reported pairs remain separate from authored phrase/config controls."""
import json
from pathlib import Path

PAIRS = [
    ("калиборд", "клипборд", "есть такое слово", [("калиборд", "клипборд")]),
    ("этабы", "табы", "добавилась лишняя э, есть слово таб", [("этабы", "табы")]),
    ("луколхоз", "локалхост", "компьютерный термин", [("луколхоз", "локалхост")]),
    ("Нет звон", "недозвон", "недозвон - термин", [("Нет звон", "недозвон")]),
    ("свуты и пнзэ", "слот и пнз", "слот и пнз - два термина", [("свуты", "слот"), ("пнзэ", "пнз")]),
    ("проковол колесо", "проколол колесо", "нет слова проковол, неправильная буква, устойчивое выражение", [("проковол", "проколол")]),
    ("долгождать", "долго ждать", "два слова слились в одно", [("долгождать", "долго ждать")]),
    ("без снятия сваты", "долго ждать", "два слова слились в одно; clarification requested", None),
    ("недозвода клиента", "недозвон до клиента", "недозвон - термин, устойчивое выражение, предлог до слился с первым словом", [("недозвода", "недозвон до")]),
]
PHRASES = [
    "долго ждать", "проколол колесо", "добрый день",
    "до свидания", "спокойной ночи", "большое спасибо", "доброе утро", "в добрый путь",
    "сломал ногу", "открыл дверь", "закрыл окно", "крепкий чай",
    "чёрный кофе", "свежий хлеб", "чистая вода", "тёплая одежда", "быстро бежать",
    "тихо говорить", "громко смеяться", "читать книгу", "писать письмо", "пить воду",
    "идти домой", "ждать ответа", "принять решение", "оказать помощь", "задать вопрос",
    "дать ответ", "обратить внимание", "иметь значение", "играть роль", "прийти вовремя",
    "потерять ключи", "найти дорогу", "сесть за стол", "выйти из дома", "пойти на работу",
    "на всякий случай", "в конце концов", "друг друга",
]
CLARIFICATION = {"source": "без снятия сваты", "previous_requested_gold": "долго ждать",
                 "requested_gold": "без снятия слота", "comment": "слот - термин",
                 "status": "resolved by user before implementation/evaluation", "date": "2026-09-21"}
CONTROLS = [
    ("долгождать ответа", [("долгождать", "долго ждать")], ["positive", "split"]),
    ("он проковол колесо", [("проковол", "проколол")], ["positive", "phrase"]),
    ("проковол\tколесо!", [("проковол", "проколол")], ["positive", "whitespace"]),
    ("добрыйдень", [("добрыйдень", "добрый день")], ["positive", "split"]),
    ("спокойнойночи", [("спокойнойночи", "спокойной ночи")], ["positive", "medium_pack"]),
    ("большоеспасибо", [("большоеспасибо", "большое спасибо")], ["positive", "medium_pack"]),
    ("питьводу", [("питьводу", "пить воду")], ["positive", "large_pack"]),
    ("принятьрешение", [("принятьрешение", "принять решение")], ["positive", "large_pack"]),
    ("свежийхлеб", [("свежийхлеб", "свежий хлеб")], ["positive", "large_pack"]),
    ("проверь этабы", [("этабы", "табы")], ["positive", "term"]),
    ("открой луколхоз", [("луколхоз", "локалхост")], ["positive", "learned_alias"]),
    ("свуты памяти", [("свуты", "слот")], ["positive", "learned_alias"]),
    ("без снятия сваты памяти", [("сваты", "слота")], ["positive", "inflection"]),
    ("долго ждать", [], ["negative", "correct_phrase"]),
    ("Долго\tждать", [], ["negative", "correct_phrase", "case"]),
    ("проколол колесо", [], ["negative", "correct_phrase"]),
    ("прокатил колесо", [], ["negative", "ordinary_words"]),
    ("прокол колеса", [], ["negative", "ordinary_words"]),
    ("долго ждал", [], ["negative", "ordinary_words"]),
    ("добрый дед", [], ["negative", "ordinary_words"]),
    ("крепкий чан", [], ["negative", "ordinary_words"]),
    ("чистая вата", [], ["negative", "ordinary_words"]),
    ("найти дороги", [], ["negative", "ordinary_words"]),
    ("этапы работы", [], ["negative", "known_word"]),
    ("табы и таб", [], ["negative", "correct_term"]),
    ("клипборд и локалхост", [], ["negative", "correct_term"]),
    ("недозвон до клиента", [], ["negative", "correct_term_phrase"]),
    ("слот и пнз", [], ["negative", "correct_term"]),
    ("сваты приехали к невесте", [], ["negative", "known_word", "inflection_collision"]),
    ("без снятия слота", [], ["negative", "correct_inflection"]),
    ("нет звонка", [], ["negative", "known_words"]),
    ("нет звона", [], ["negative", "known_words"]),
    ("обсуждаем выражение нет звон", [], ["negative", "literal_alias"]),
    ("слово этабы содержит ошибку", [], ["negative", "literal_alias"]),
    ("слово луколхоз смешное", [], ["negative", "literal_alias"]),
    ("`луколхоз`", [], ["negative", "code"]),
    ("https://host/этабы", [], ["negative", "url"]),
    ("ПНЗ и СЛОТ", [], ["negative", "uppercase"]),
    ("проковол\nколесо", [], ["negative", "newline"]),
    ("проковол, колесо", [], ["negative", "punctuation"]),
    ("нет\nзвон", [], ["negative", "newline"]),
    ("долгождать.txt", [], ["negative", "file"]),
    ("проколол  колесо", [], ["negative", "whitespace"]),
]


def record(ident, source, changes, tags, real=False):
    edits = []; parts = []; cursor = 0
    for original, replacement in changes or []:
        start = source.index(original, cursor); end = start + len(original)
        token_start = len(source[:start].split())
        edits.append({"byte_start": len(source[:start].encode()), "byte_end": len(source[:end].encode()),
                      "token_start": token_start, "token_end": token_start + len(original.split()), "replacement": replacement})
        parts.extend([source[cursor:start], replacement]); cursor = end
    parts.append(source[cursor:])
    return {"schema_version": 1, "id": ident, "group_id": ident, "split": "dev", "source": source,
            "gold": "".join(parts) if changes is not None else None, "edits": edits, "tags": tags,
            "provenance": {"kind": "real_asr" if real else "synthetic", "asr_model": None,
                "origin": "user-reported ASR pair in conversation 2026-09-21; audio/model unavailable; development data" if real else "authored 010 phrase/config controls before implementation; not independent holdout"},
            "annotation_status": "reviewed" if changes is not None else "ambiguous"}


def main():
    root = Path(__file__).resolve().parents[1]
    user = root / "data/user-examples/2026-09-21"; user.mkdir(parents=True)
    synthetic = root / "data/fixtures/user-phrases"; synthetic.mkdir(parents=True)
    with (user / "raw-pairs.json").open("x", encoding="utf-8") as f:
        json.dump([{"source": s, "requested_gold": g, "comment": comment, "clarification_required": edits is None}
                   for s, g, comment, edits in PAIRS], f, ensure_ascii=False, indent=2); f.write("\n")
    with (user / "clarifications.json").open("x", encoding="utf-8") as f:
        json.dump([CLARIFICATION], f, ensure_ascii=False, indent=2); f.write("\n")
    with (user / "corpus.jsonl").open("x", encoding="utf-8") as f:
        for i, (source, gold, _, edits) in enumerate(PAIRS, 1):
            if source == CLARIFICATION["source"]:
                gold = CLARIFICATION["requested_gold"]; edits = [("сваты", "слота")]
            r = record(f"user-20260921-{i:03}", source, edits, ["user_reported"], True)
            assert edits is None or r["gold"] == gold
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    with (synthetic / "corpus.jsonl").open("x", encoding="utf-8") as f:
        for i, (source, edits, tags) in enumerate(CONTROLS, 1):
            f.write(json.dumps(record(f"phrase-control-{i:03}", source, edits, tags), ensure_ascii=False) + "\n")
    with (synthetic / "phrase-packs.json").open("x", encoding="utf-8") as f:
        json.dump({"sizes": {"small": 3, "medium": 12, "large": 40}, "phrases": PHRASES,
                   "scope": "nested authored controls, not a general Russian phrase dictionary"}, f, ensure_ascii=False, indent=2); f.write("\n")


if __name__ == "__main__":
    main()
