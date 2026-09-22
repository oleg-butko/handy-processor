"""020 frozen controls and settings; no threshold fitting."""
from copy import deepcopy
from composition_profiles import EVENTS as OLD_EVENTS, corpus as old_corpus, scenarios as old_scenarios, bindings as old_bindings
from prepare_draft_fixture import correction
from prepare_user_phrase_fixture import record

EVENTS = [*deepcopy(OLD_EVENTS), correction("pnz", "проверь пэ эн зэ сегодня", "проверь ПНЗ сегодня")]
EXTRA = [
    ("открой калиборт", [("калиборт", "клипборд")]),
    ("проверь пэ эн зэ сегодня", [("пэ эн зэ", "ПНЗ")]),
    ("проверь пээнзэ сегодня", [("пээнзэ", "ПНЗ")]),
    ("проверь пэ энзэ сегодня", [("пэ энзэ", "ПНЗ")]),
    ("проверь пэ эн зэ завтра", [("пэ эн зэ", "ПНЗ")]),
    ("обсудим написание пэ эн зэ", []),
    ("полезный свод правил", []), ("ввод данных", []),
    ("сваты приехали вчера", []), ("сняли свод\nпамяти", []),
]


def corpus():
    rows = old_corpus()
    for i, (source, edits) in enumerate(EXTRA, 1):
        row = record(f"mixed-{i:03}", source, edits, ["synthetic", "mixed-generation", "positive" if edits else "negative"])
        row["provenance"]["origin"] = "020 authored generation controls; split PNZ variants remain positive gold"
        rows.append(row)
    return rows


def scenarios():
    old = old_scenarios()
    names = ["manual-exact", "exact", "manual-edit", "mixed", "weighted", "unpinned",
             "feedback-off", "binding-off", "owner-off", "set-off", "global-off", "owner-low", "new-low",
             "conflict", "owner-high-conflict", "known-never", "excluded", "undo", "duplicate", "source-penalty"]
    out = {}
    for name in names:
        original = "manual-only" if name.startswith("manual-") else name if name in old else "composed"
        config, settings, _ = deepcopy(old[original])
        config["policy"]["mode"] = "exact" if name in ["manual-exact", "exact"] else "edit"
        if name == "weighted":
            # Same uniform 100-unit distance-1 model as 013, no new fitted costs.
            config["policy"]["weighted_distance"] = dict(max_cost=100, insertion=100, deletion=100, substitutions=[])
        events = deepcopy(EVENTS)
        if name == "undo": events.extend(old["undo"][2][len(OLD_EVENTS):])
        if name == "duplicate": events.append(deepcopy(EVENTS[0]))
        out[name] = (config, settings, events)
    return out


def bindings(proposals, name):
    if name.startswith("manual-"): return []
    rows = old_bindings([p for p in proposals if p["canonical"] != "ПНЗ"], name)
    rows += [dict(proposal_id=p["id"], enabled=True, owner=None, boost=-5.0 if name == "new-low" else 0.0)
             for p in proposals if p["canonical"] == "ПНЗ"]
    return rows
