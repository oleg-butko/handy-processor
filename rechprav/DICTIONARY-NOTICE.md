# Словарь русских словоформ

`artifacts/opencorpora-wordfreq/dictionary.rpt` содержит преобразованные данные
OpenCorpora (https://opencorpora.org/), полученные из пакета
`pymorphy3-dicts-ru==2.4.417150.4580142`, Danylo Halaiko:
https://pypi.org/project/pymorphy3-dicts-ru/2.4.417150.4580142/.

Данные распространяются по **Creative Commons Attribution-ShareAlike 3.0**:
https://creativecommons.org/licenses/by-sa/3.0/
Текст лицензии: https://creativecommons.org/licenses/by-sa/3.0/legalcode.
MIT-лицензия Python-обёртки не заменяет лицензию словарных данных.

Изменения в Handy Processor: экспорт хранимых словоформ, фильтрация по русским
буквам (1–128 символов), удаление повторов, нормализация е/ё и упаковка в индекс
RPT. Снимок OpenCorpora: source revision 417150, corpus revision 4580142,
дата сборки upstream 2022-01-08. Подготовка описана в
`docs/rechprav/resource-sources.md` и `docs/rechprav/packed-index.md` исходного
проекта; код преобразований — `rechprav/scripts/prepare_resources.py` и
`rechprav/src/packed.rs`.

Частотные данные wordfreq в этот runtime-комплект не входят.
