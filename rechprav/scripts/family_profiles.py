"""011: grouped forms versus equivalent explicit flat records."""
import copy
from user_phrase_profiles import profiles as previous_profiles


def profiles(root):
    old = previous_profiles(root)["learned-safe"]
    complete = copy.deepcopy(old)
    words = complete["sets"][1]["terms"]
    tab = copy.deepcopy(next(t for t in words if t["id"] == "tabs"))
    tab.update(id="tab", canonical="таб", aliases=["таб"])
    locative = copy.deepcopy(next(t for t in words if t["id"] == "slot-genitive"))
    locative.update(id="slot-locative", canonical="слоте", aliases=["слоте", "свате"], context_patterns=["в {term}"])
    words.extend([tab, locative])
    grouped = copy.deepcopy(complete)
    words = grouped["sets"][1]["terms"]
    by_id = {t["id"]: t for t in words}
    for base, members in [("slot", ["slot-genitive", "slot-locative"]), ("tab", ["tabs"])]:
        by_id[base]["forms"] = [{key: copy.deepcopy(value) for key, value in by_id[member].items()
                                if key in ["canonical", "aliases", "context", "context_patterns", "context_exclusions", "require_context"]}
                               for member in members]
    grouped["sets"][1]["terms"] = [t for t in words if t["id"] not in ["slot-genitive", "slot-locative", "tabs"]]
    result = {"old-flat": old, "flat-complete": complete}
    for name in ["family", "family-low", "family-high", "family-slot-off", "family-no-genitive", "family-base-only",
                 "family-set-off", "family-conflict", "family-preferred", "family-no-form-context"]:
        c = copy.deepcopy(grouped); user = c["sets"][1]; slot = next(t for t in user["terms"] if t["id"] == "slot")
        if name in ["family-low", "family-high"]: slot["boost"] += -2.0 if name == "family-low" else 2.0
        if name == "family-slot-off": slot["enabled"] = False
        if name == "family-set-off": user["enabled"] = False
        if name == "family-no-genitive": slot["forms"] = [f for f in slot["forms"] if f["canonical"] != "слота"]
        if name == "family-base-only":
            for t in user["terms"]: t.pop("forms", None)
        if name == "family-no-form-context":
            for f in slot["forms"]:
                f.pop("context_patterns", None); f.pop("require_context", None)
        if name in ["family-conflict", "family-preferred"]:
            rival = {key: copy.deepcopy(value) for key, value in slot.items() if key != "forms"}
            rival.update(id="rival", canonical="свитч", aliases=["свуты", "сваты", "свате", "этабы"])
            c["sets"].append({"id":"conflicting", "enabled":True, "boost":0.0, "terms":[rival]})
        if name == "family-preferred": user["boost"] = 1.0
        result[name] = c
    return result
