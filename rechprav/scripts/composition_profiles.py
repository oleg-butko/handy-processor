"""017 frozen manual settings, events, controls and ablations."""
from copy import deepcopy
from prepare_draft_fixture import correction, retract
from prepare_user_phrase_fixture import record

EVENTS = [correction("slot", "сняли свод памяти", "сняли слот памяти"),
          correction("form", "без снятия сваты", "без снятия слота"),
          correction("new", "включи катэхэ", "включи КТХ"),
          correction("phrase", "надо долгождать ответа", "надо долго ждать ответа")]
CASES = [
    ("сняли свод памяти", [("свод", "слот")]),
    ("вчера сняли свод памяти", [("свод", "слот")]),
    ("обсуждаем выражение сняли свод памяти", []),
    ("свод правил", []), ("сняли свод арки", []),
    ("сняли свуты", [("свуты", "слот")]),
    ("без снятия сваты", [("сваты", "слота")]),
    ("после снятия сваты", []), ("сваты приехали", []),
    ("включи катэхэ", [("катэхэ", "КТХ")]), ("слово катэхэ", []),
    ("надо долгождать ответа", [("долгождать", "долго ждать")]),
    ("долгождать", []), ("открой калиборд", [("калиборд", "клипборд")]),
    ("слот слота клипборд", []), ("сняли\nсвод памяти", []),
    ("сняли, свод памяти", []), ("сняли `свод` памяти", []),
    ("https://host/калиборд", []), ("включи катэхи", []),
]


def manual():
    return dict(baseline_config="opencorpora-packed-frequency-b2.toml", enabled=True,
                policy=dict(mode="exact"), sets=[dict(id="hardware", enabled=True, boost=0.0, terms=[
                    dict(id="slot", canonical="слот", aliases=["свуты"], boost=0.0, known_words="always", max_known_distance=0,
                         forms=[dict(canonical="слота", aliases=["слота"], require_context=True, context_patterns=["без снятия {term}"])]),
                    dict(id="clipboard", canonical="клипборд", aliases=["калиборд"], known_words="always", max_known_distance=0)]),
                    dict(id="phrases", terms=[dict(id="wait", canonical="долго ждать", kind="phrase", aliases=["долго ждать"], known_words="preserve")])])


def scenarios():
    names = ["manual-only", "composed", "feedback-off", "binding-off", "owner-off", "set-off", "global-off",
             "owner-low", "new-low", "conflict", "owner-high-conflict", "known-never", "excluded",
             "disjoint-context", "unsupported-cues", "unsupported-edit", "undo", "duplicate",
             "owner-deleted", "new-over-existing", "source-penalty"]
    out = {}
    for name in names:
        config = manual()
        slot = config["sets"][0]["terms"][0]
        settings = dict(enabled=True, bindings=[])
        events = deepcopy(EVENTS)
        if name == "feedback-off": settings["enabled"] = False
        if name == "owner-off": slot["enabled"] = False
        if name == "set-off": config["sets"][0]["enabled"] = False
        if name == "global-off": config["enabled"] = False
        if name in ["owner-low", "source-penalty"]: slot["boost"] = -5.0
        if name == "owner-high-conflict": slot["boost"] = 3.0
        if name in ["conflict", "owner-high-conflict"]:
            config["sets"].append(dict(id="network", terms=[dict(id="switch", canonical="свитч", aliases=["свод"], known_words="always", require_context=True, context_patterns=["сняли {term} памяти"], max_known_distance=0)]))
        if name == "known-never": slot["known_words"] = "never"
        if name == "excluded": slot["context_exclusions"] = ["сняли {term} памяти"]
        if name == "disjoint-context":
            slot.update(require_context=True, context_patterns=["вставили {term}"])
        if name == "unsupported-cues": slot["context"] = ["памяти"]
        if name == "unsupported-edit": config["policy"]["mode"] = "edit"
        if name == "undo": events += [retract("undo-slot", "slot")]
        if name == "duplicate": events += [deepcopy(EVENTS[0])]
        if name == "owner-deleted": config["sets"][0]["terms"].pop(0)
        if name == "source-penalty": config["source_penalties"] = [dict(id="source", source="свод", penalty=5.0, context_patterns=["сняли {term} памяти"])]
        out[name] = (config, settings, events)
    return out


def bindings(proposals, name):
    owners = {"слот":dict(set_id="hardware", term_id="slot"), "слота":dict(set_id="hardware", term_id="slot"),
              "долго ждать":dict(set_id="phrases", term_id="wait"), "КТХ":None}
    if name == "manual-only": return []
    return [dict(proposal_id=p["id"], enabled=not (name == "binding-off" and p["canonical"] == "слот"),
                 owner=None if name == "new-over-existing" and p["canonical"] == "слот" else owners[p["canonical"]],
                 boost=-5.0 if name == "new-low" and p["canonical"] == "КТХ" else 0.0) for p in proposals]


def corpus():
    rows = [record(f"compose-{i:03}", source, edits, ["synthetic", "composition", "positive" if edits else "negative"])
            for i, (source, edits) in enumerate(CASES, 1)]
    for r in rows: r["provenance"]["origin"] = "017 authored composition controls; dev only"
    return rows
