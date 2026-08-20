import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { normalizeRussianNumerals } from "./russian-numerals.js";

describe("normalizeRussianNumerals", () => {
  it("converts units, gender forms, and zero", () => {
    assert.equal(normalizeRussianNumerals("ноль один одна одно два две"), "0 1 1 1 2 2");
  });

  it("converts compound numbers through 999", () => {
    assert.equal(normalizeRussianNumerals("двадцать два"), "22");
    assert.equal(normalizeRussianNumerals("сто пять"), "105");
    assert.equal(normalizeRussianNumerals("девятьсот девяносто девять"), "999");
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
  });

  it("does not change words that merely contain a numeral", () => {
    assert.equal(normalizeRussianNumerals("трибуна и семья"), "трибуна и семья");
  });
});
