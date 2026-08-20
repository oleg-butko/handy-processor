#!/usr/bin/env python3
"""Generate the TypeScript dictionary used by the Russian numeral parser.

Python and pymorphy3 are development-time tools only. The generated
TypeScript file is checked into Git and has no Python dependency at runtime.
"""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Literal


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "src" / "generated" / "russian-numerals.ts"

NumeralKind = Literal["unit", "teen", "ten", "hundred", "scale"]


@dataclass(frozen=True)
class NumeralSpec:
    canonical: str
    value: int
    kind: NumeralKind
    forms: tuple[str, ...]
    lemmas: tuple[str, ...] = ()


def numeral(
    canonical: str,
    value: int,
    kind: NumeralKind,
    *forms: str,
    lemmas: tuple[str, ...] = (),
) -> NumeralSpec:
    return NumeralSpec(canonical, value, kind, (canonical, *forms), lemmas)


# This is an intentionally reviewed allowlist rather than every parse returned
# by a morphology engine. Homonyms that are unsafe without numeric context are
# listed separately in AMBIGUOUS_FORMS.
NUMERALS = (
    numeral(
        "ноль",
        0,
        "unit",
        "нуль",
        "ноля",
        "нуля",
        "нолей",
        "нулей",
        "нолю",
        "нулю",
        "нолем",
        "нулем",
        "ноле",
        "нуле",
        "нолях",
        "нулях",
        lemmas=("ноль", "нуль"),
    ),
    numeral("один", 1, "unit", "одна", "одно", "одни", "одного", "одной", "одних", "одному", "одним", "одну", "одними"),
    numeral("два", 2, "unit", "две", "двух", "двум", "двумя"),
    numeral("три", 3, "unit", "трех", "трем", "тремя"),
    numeral("четыре", 4, "unit", "четырех", "четырем", "четырьмя"),
    numeral("пять", 5, "unit", "пяти", "пятью"),
    numeral("шесть", 6, "unit", "шести", "шестью"),
    numeral("семь", 7, "unit", "семи", "семью"),
    numeral("восемь", 8, "unit", "восьми", "восемью"),
    numeral("девять", 9, "unit", "девяти", "девятью"),
    numeral("десять", 10, "teen", "десяти", "десятью"),
    numeral("одиннадцать", 11, "teen", "одиннадцати", "одиннадцатью"),
    numeral("двенадцать", 12, "teen", "двенадцати", "двенадцатью"),
    numeral("тринадцать", 13, "teen", "тринадцати", "тринадцатью"),
    numeral("четырнадцать", 14, "teen", "четырнадцати", "четырнадцатью"),
    numeral("пятнадцать", 15, "teen", "пятнадцати", "пятнадцатью"),
    numeral("шестнадцать", 16, "teen", "шестнадцати", "шестнадцатью"),
    numeral("семнадцать", 17, "teen", "семнадцати", "семнадцатью"),
    numeral("восемнадцать", 18, "teen", "восемнадцати", "восемнадцатью"),
    numeral("девятнадцать", 19, "teen", "девятнадцати", "девятнадцатью"),
    numeral("двадцать", 20, "ten", "двадцати", "двадцатью"),
    numeral("тридцать", 30, "ten", "тридцати", "тридцатью"),
    numeral("сорок", 40, "ten", "сорока"),
    numeral("пятьдесят", 50, "ten", "пятидесяти", "пятьюдесятью"),
    numeral("шестьдесят", 60, "ten", "шестидесяти", "шестьюдесятью"),
    numeral("семьдесят", 70, "ten", "семидесяти", "семьюдесятью"),
    numeral("восемьдесят", 80, "ten", "восьмидесяти", "восемьюдесятью"),
    numeral("девяносто", 90, "ten", "девяноста"),
    numeral("сто", 100, "hundred", "ста"),
    numeral("двести", 200, "hundred", "двухсот", "двумстам", "двумястами", "двухстах"),
    numeral("триста", 300, "hundred", "трехсот", "тремстам", "тремястами", "трехстах"),
    numeral("четыреста", 400, "hundred", "четырехсот", "четыремстам", "четырьмястами", "четырехстах"),
    numeral("пятьсот", 500, "hundred", "пятисот", "пятистам", "пятьюстами", "пятистах"),
    numeral("шестьсот", 600, "hundred", "шестисот", "шестистам", "шестьюстами", "шестистах"),
    numeral("семьсот", 700, "hundred", "семисот", "семистам", "семьюстами", "семистах"),
    numeral("восемьсот", 800, "hundred", "восьмисот", "восьмистам", "восемьюстами", "восьмистах"),
    numeral("девятьсот", 900, "hundred", "девятисот", "девятистам", "девятьюстами", "девятистах"),
    numeral("тысяча", 1_000, "scale", "тысячи", "тысяч", "тысяче", "тысячу", "тысячей", "тысячью", "тысячам", "тысячами", "тысячах"),
    numeral("миллион", 1_000_000, "scale", "миллиона", "миллионов", "миллиону", "миллионам", "миллионом", "миллионами", "миллионе", "миллионах"),
    numeral("миллиард", 1_000_000_000, "scale", "миллиарда", "миллиардов", "миллиарду", "миллиардам", "миллиардом", "миллиардами", "миллиарде", "миллиардах"),
    numeral("триллион", 1_000_000_000_000, "scale", "триллиона", "триллионов", "триллиону", "триллионам", "триллионом", "триллионами", "триллионе", "триллионах"),
)

AMBIGUOUS_FORMS = frozenset({"одно", "одни", "трем", "семью", "сорока"})


def normalize_word(word: str) -> str:
    return word.lower().replace("ё", "е")


def build_dictionary() -> dict[str, NumeralSpec]:
    entries: dict[str, NumeralSpec] = {}

    for spec in NUMERALS:
        for form in spec.forms:
            word = normalize_word(form)
            previous = entries.get(word)
            if previous is not None and (
                previous.value != spec.value or previous.kind != spec.kind
            ):
                raise ValueError(
                    f"form {word!r} maps to both {previous.value} and {spec.value}"
                )
            entries[word] = spec

    missing_ambiguous_forms = AMBIGUOUS_FORMS.difference(entries)
    if missing_ambiguous_forms:
        raise ValueError(
            "ambiguous forms are missing from the dictionary: "
            + ", ".join(sorted(missing_ambiguous_forms))
        )

    return entries


def validate_morphology() -> None:
    try:
        import pymorphy3
    except ImportError as error:
        raise RuntimeError(
            "pymorphy3 is required for --validate-morphology; "
            "install requirements-generator.txt in a virtual environment"
        ) from error

    analyzer = pymorphy3.MorphAnalyzer(lang="ru")
    errors: list[str] = []

    for spec in NUMERALS:
        expected_lemmas = set(spec.lemmas or (spec.canonical,))
        for form in spec.forms:
            parses = analyzer.parse(form)
            if not any(parse.normal_form in expected_lemmas for parse in parses):
                errors.append(f"{form!r} is not recognized as {spec.canonical!r}")

    if errors:
        raise RuntimeError("morphology validation failed:\n" + "\n".join(errors))


def render() -> str:
    lines = [
        "// This file is generated by scripts/generate_russian_numerals.py.",
        "// Do not edit it manually.",
        "",
        'export type RussianNumeralKind = "unit" | "teen" | "ten" | "hundred" | "scale";',
        "",
        "export interface RussianNumeralEntry {",
        "  readonly value: number;",
        "  readonly kind: RussianNumeralKind;",
        "  readonly canonical: string;",
        "  readonly ambiguous?: true;",
        "}",
        "",
        "export const RUSSIAN_NUMERAL_WORDS = {",
    ]

    for word, spec in sorted(build_dictionary().items()):
        encoded_word = json.dumps(word, ensure_ascii=False)
        canonical = json.dumps(spec.canonical, ensure_ascii=False)
        ambiguous = ", ambiguous: true" if word in AMBIGUOUS_FORMS else ""
        lines.append(
            f'  {encoded_word}: {{ value: {spec.value}, kind: "{spec.kind}", '
            f"canonical: {canonical}{ambiguous} }},"
        )

    lines.extend(
        [
            "} as const satisfies Readonly<Record<string, RussianNumeralEntry>>;",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail if the checked-in TypeScript dictionary is stale",
    )
    parser.add_argument(
        "--validate-morphology",
        action="store_true",
        help="validate the curated forms with pymorphy3 before generating",
    )
    args = parser.parse_args()

    if args.validate_morphology:
        try:
            validate_morphology()
        except RuntimeError as error:
            parser.error(str(error))

    generated = render()

    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text(encoding="utf-8") != generated:
            parser.error(f"{OUTPUT.relative_to(ROOT)} is stale; run this script")
        return 0

    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(generated, encoding="utf-8", newline="\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
