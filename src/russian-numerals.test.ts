import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { normalizeRussianNumerals } from "./russian-numerals.js";

describe("normalizeRussianNumerals", () => {
  it("matches uppercase numerals without changing surrounding casing", () => {
    assert.equal(
      normalizeRussianNumerals("МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА"),
      "МНЕ НУЖНО 22 ФАЙЛА",
    );
  });

  it("converts units, gender forms, and zero", () => {
    assert.equal(normalizeRussianNumerals("ноль один одна два две"), "0 1 1 2 2");
  });

  it("converts compound numbers through 999", () => {
    assert.equal(normalizeRussianNumerals("двадцать два"), "22");
    assert.equal(normalizeRussianNumerals("сто пять"), "105");
    assert.equal(normalizeRussianNumerals("девятьсот девяносто девять"), "999");
  });

  it("converts common case forms", () => {
    assert.equal(normalizeRussianNumerals("двадцати двух"), "22");
    assert.equal(normalizeRussianNumerals("двумястами двадцатью двумя"), "222");
    assert.equal(normalizeRussianNumerals("трёхсот сорока пяти"), "345");
  });

  it("uses ambiguous forms only inside a valid compound number", () => {
    assert.equal(normalizeRussianNumerals("сто одно"), "101");
    assert.equal(normalizeRussianNumerals("сорока двух"), "42");
    assert.equal(normalizeRussianNumerals("трем тысячам"), "3000");
    assert.equal(normalizeRussianNumerals("семью тысячами"), "7000");
  });

  it("preserves ambiguous forms without numeric context", () => {
    assert.equal(normalizeRussianNumerals("сорока сидит на ветке"), "сорока сидит на ветке");
    assert.equal(normalizeRussianNumerals("мы трем сыр"), "мы трем сыр");
    assert.equal(normalizeRussianNumerals("вокруг одни проблемы"), "вокруг одни проблемы");
    assert.equal(normalizeRussianNumerals("это одно и то же"), "это одно и то же");
    assert.equal(normalizeRussianNumerals("я люблю свою семью"), "я люблю свою семью");
  });

  it("converts thousands and millions", () => {
    assert.equal(normalizeRussianNumerals("тысяча сто"), "1100");
    assert.equal(normalizeRussianNumerals("две тысячи двадцать два"), "2022");
    assert.equal(normalizeRussianNumerals("миллион сто тысяч сто"), "1100100");
    assert.equal(
      normalizeRussianNumerals("двух миллионов трехсот тысяч пяти"),
      "2300005",
    );
  });

  it("uses bigint arithmetic for billion and trillion scales", () => {
    assert.equal(
      normalizeRussianNumerals(
        "девятьсот триллионов восемьсот миллиардов семьсот миллионов шестьсот тысяч пятьсот",
      ),
      "900800700600500",
    );
  });

  it("corrects one unambiguous edit next to an exact numeral", () => {
    assert.equal(normalizeRussianNumerals("двацать два"), "22");
    assert.equal(normalizeRussianNumerals("двадццать два"), "22");
    assert.equal(normalizeRussianNumerals("дважцать два"), "22");
    assert.equal(normalizeRussianNumerals("двацдать два"), "22");
    assert.equal(normalizeRussianNumerals("два миллиноа"), "2000000");
  });

  it("does not fuzzy-correct words without safe numeric context", () => {
    assert.equal(normalizeRussianNumerals("двацать файлов"), "двацать файлов");
    assert.equal(normalizeRussianNumerals("двацать, два"), "двацать, 2");
    assert.equal(normalizeRussianNumerals("пять тесть"), "5 тесть");
    assert.equal(normalizeRussianNumerals("осемнадцати двух"), "осемнадцати 2");
  });

  it("replaces numeral spans inside ordinary text", () => {
    assert.equal(
      normalizeRussianNumerals("мне нужно двадцать два файла и три каталога"),
      "мне нужно 22 файла и 3 каталога",
    );
  });

  it("keeps punctuation and whitespace around replacements", () => {
    assert.equal(normalizeRussianNumerals("двадцать два,  три!"), "22,  3!");
  });

  it("does not combine invalid adjacent number groups", () => {
    assert.equal(normalizeRussianNumerals("сто двести триста"), "100 200 300");
    assert.equal(normalizeRussianNumerals("один два"), "1 2");
    assert.equal(normalizeRussianNumerals("двадцать тридцать"), "20 30");
    assert.equal(normalizeRussianNumerals("тысяча миллион"), "1000 1000000");
  });

  it(
    "handles long invalid numeral sequences in bounded time",
    { timeout: 2_000 },
    () => {
      const input = Array<string>(2_000).fill("один").join(" ");
      const expected = Array<string>(2_000).fill("1").join(" ");
      assert.equal(normalizeRussianNumerals(input), expected);
    },
  );

  it("does not change words that merely contain a numeral", () => {
    assert.equal(normalizeRussianNumerals("трибуна и семья"), "трибуна и семья");
  });
});
