"""015 frozen lifecycle cases. Recipes select review tokens, never fabricate evidence."""
from copy import deepcopy
from prepare_draft_fixture import EVENTS, correction, retract


def scenarios():
    initial = deepcopy(EVENTS)
    repeated = initial + [correction("repeat-slot", "сняли свод памяти", "сняли слот памяти")]
    one_left = repeated + [retract("undo-slot", "slot")]
    gone = one_left + [retract("undo-repeat", "repeat-slot")]
    reborn = gone + [correction("new-slot", "сняли свод памяти", "сняли слот памяти")]
    context = gone + [correction("context-slot", "освободили свод памяти", "освободили слот памяти")]
    conflict = initial + [correction("rival", "сняли свод памяти", "сняли свитч памяти")]
    snapshots = dict(initial=initial, repeated=repeated, one_left=one_left,
                     gone=gone, reborn=reborn, context=context, conflict=conflict)
    # (snapshot, scope, canonical selector '*' or exact string, action)
    accept = [("initial", "technical", "*", "accept")]
    two = [("repeated", "technical", "*", "accept")]
    cases = {
        "unreviewed": ("initial", [], "technical", 0),
        "accepted": ("initial", accept, "technical", 8),
        "duplicate-decision": ("initial", accept, "technical", 8),
        "unapproved-repeat": ("one_left", accept, "technical", 7),
        "approved-repeat": ("one_left", two, "technical", 8),
        "undo-last": ("gone", two, "technical", 7),
        "reborn-stale": ("reborn", two, "technical", 7),
        "reborn-accepted": ("reborn", two + [("reborn", "technical", "слот", "accept")], "technical", 8),
        "disabled": ("initial", accept + [("initial", "technical", "слот", "disable")], "technical", 7),
        "rejected": ("initial", accept + [("initial", "technical", "слот", "reject")], "technical", 7),
        "reenabled": ("initial", accept + [("initial", "technical", "слот", "disable"), ("initial", "technical", "слот", "accept")], "technical", 8),
        "new-context": ("context", two, "technical", 7),
        "other-unreviewed": ("initial", accept, "other", 0),
        "other-accepted": ("initial", accept + [("initial", "other", "*", "accept")], "other", 1),
        "conflict-unapproved": ("conflict", accept, "technical", 8),
        "conflict-accepted": ("conflict", accept + [("conflict", "technical", "свитч", "accept")], "technical", 9),
    }
    return snapshots, cases
