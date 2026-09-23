# 023: дополнительное ревью

Дата: 2026-09-23. Baseline: `a199ba8`.
Ветка: `fix/rechprav-additional-review`.

[Разбор ошибок и исправлений](../../../docs/rechprav/additional-review.md).

| Проверка | Команда | Результат / вывод |
|---|---|---|
| Воспроизведение до исправлений, первая версия теста | `node --test --test-name-pattern='malformed Unicode\|same batch' dist/correction.test.js` | [2 отказа, также обнаружен необработанный отказ соседнего запроса](regressions-before.log) |
| Воспроизведение до исправлений, сбор обоих Promise | Та же команда после изменения теста на `Promise.allSettled` | [2 отказа без unhandled rejection](regressions-before-v2.log) |
| Сборка перед воспроизведением | `npm run build:server` | [Успешно](build-before.log) |
| Сборка исправленной версии | `npm run build:server` | [Успешно](build-server.log) |
| Тесты корректора | `node --test dist/correction.test.js` | [12/12](correction-tests.log) |
| Весь серверный набор | `node --test dist/*.test.js` | [100/100](server-tests.log) |
| Проверка типов | `npm run typecheck` | [Сервер и UI прошли](typecheck.log) |
| Браузерная интеграция корректора | `node --test ui/tests/correction.browser.test.mjs` | [1/1](browser-correction.log) |

Серверный набор включает 12 тестов корректора: эти числа не суммируются.
Использованы существующий release worker, временные словари и fake worker для
точного воспроизведения нарушения протокола. Алгоритм и ресурсы не менялись.
Полные UI/Rust/packaging-наборы и замеры качества в этом ревью не запускались.
