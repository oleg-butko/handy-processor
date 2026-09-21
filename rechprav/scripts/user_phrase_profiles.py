"""Frozen 010 config changes. Word forms are explicit entries, not morphology."""
import copy
import json
import tomllib


def profiles(root):
    original = root / "configs/terms-pronunciation-manual.toml"
    seed = tomllib.loads(original.read_text())
    base = {"baseline_config": str((original.parent / seed["baseline_config"]).resolve()),
            "enabled": True, "policy": seed["policy"], "sets": []}
    packs = json.loads((root / "data/fixtures/user-phrases/phrase-packs.json").read_text())
    def term(ident, canonical, cap, phrase=False):
        result = {"id": ident, "canonical": canonical, "aliases": [canonical], "enabled": True,
                  "boost": 1.0, "known_words": "always", "context": [], "max_known_distance": cap}
        if phrase: result["kind"] = "phrase"
        return result
    ordinary = {"id": "russian_phrases", "enabled": True, "boost": 0.0,
                "terms": [term(f"p{i:03}", text, 1, True) for i, text in enumerate(packs["phrases"], 1)]}
    words = [("clipboard", "клипборд", "калиборд"), ("tabs", "табы", "этабы"),
             ("localhost", "локалхост", "луколхоз"), ("noanswer", "недозвон", "нет звон"),
             ("slot", "слот", "свуты"), ("slot-genitive", "слота", "сваты"),
             ("pnz", "пнз", "пнзэ"), ("noanswer-to", "недозвон до", "недозвода")]
    user = {"id": "user", "enabled": True, "boost": 0.0,
            "terms": [term(ident, canonical, 0, ' ' in canonical) for ident, canonical, _ in words]}
    result = {}
    for name in ["disabled", "phrases-only", "canonical-small", "canonical-medium", "canonical-large",
                 "canonical-large-distance2", "learned-large", "learned-low", "learned-high", "learned-context", "learned-safe",
                 "user-set-off", "slot-term-off", "conflict", "prefer-user"]:
        config = copy.deepcopy(base); config["sets"] = [copy.deepcopy(ordinary), copy.deepcopy(user)]
        size = packs["sizes"].get(name.removeprefix("canonical-"), 40)
        config["sets"][0]["terms"] = config["sets"][0]["terms"][:size]
        learned = name.startswith("learned-") or name in ["user-set-off", "slot-term-off", "conflict", "prefer-user"]
        if learned:
            for t, (_, _, alias) in zip(config["sets"][1]["terms"], words, strict=True): t["aliases"].append(alias)
        if name == "disabled": config["enabled"] = False
        if name in ["phrases-only", "user-set-off"]: config["sets"][1]["enabled"] = False
        if name == "canonical-large-distance2": config["policy"]["max_distance"] = 2
        if name in ["learned-low", "learned-high"]:
            for t in config["sets"][1]["terms"]: t["boost"] += -2.0 if name == "learned-low" else 2.0
        if name in ["learned-context", "learned-safe"]:
            t = config["sets"][1]["terms"][5]
            t["require_context"] = True; t["context_patterns"] = ["без снятия {term}"]
        if name == "learned-safe":
            for t in config["sets"][0]["terms"]: t["known_words"] = "preserve"
        if name == "slot-term-off":
            for t in config["sets"][1]["terms"]:
                if t["id"] in ["slot", "slot-genitive"]: t["enabled"] = False
        if name in ["conflict", "prefer-user"]:
            rival = term("rival", "свитч", 0); rival["aliases"] = ["свуты", "пнзэ"]
            config["sets"].append({"id": "conflicting", "enabled": True, "boost": 0.0, "terms": [rival]})
        if name == "prefer-user": config["sets"][1]["boost"] = 1.0
        result[name] = config
    return result
