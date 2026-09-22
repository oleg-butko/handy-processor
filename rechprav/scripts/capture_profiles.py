"""016 frozen editor fragment requests and expected correction/retract events."""
from prepare_draft_fixture import correction, retract


def replace(id, fragment, before, after, revision=0, scope="technical", confirmed=True, selection=None):
    request = dict(schema_version=1, request_id=id, fragment_id=fragment, session_id="window-1",
                   scope=scope, expected_revision=revision, kind="replace", before=before, after=after,
                   confirmed_asr=confirmed)
    if selection is not None:
        request["selection"] = selection
    return request


def fixture():
    requests = [
        replace("slot", "slot-fragment", "сняли свод памяти", "сняли слот памяти"),
        replace("clipboard", "clipboard-fragment", "открой калиборд", "открой клипборд"),
        replace("split", "split-fragment", "надо долгождать ответа", "надо долго ждать ответа"),
        replace("pnz", "pnz-fragment", "включи пензэ", "включи ПНЗ"),
        replace("style", "style-fragment", "очень хороший день", "прекрасный день", confirmed=False),
        replace("other", "other-fragment", "сняли свод памяти", "сняли свитч памяти", scope="other"),
        replace("emoji", "emoji-fragment", "😀 сняли свод памяти", "😀 сняли слот памяти",
                selection=dict(start_utf16=9, end_utf16=13, replacement="слот")),
    ]
    expected = [correction(r["request_id"], r["before"], r["after"], r["scope"], r["confirmed_asr"],
                           **({"selection": dict(byte_start=16, byte_end=24, replacement="слот")} if r["request_id"] == "emoji" else {}))
                for r in requests]
    undo = dict(schema_version=1, request_id="undo-slot", fragment_id="slot-fragment", session_id="window-1",
                scope="technical", expected_revision=1, kind="undo", before="сняли слот памяти", after="сняли свод памяти", target_request_id="slot")
    undo_emoji = dict(schema_version=1, request_id="undo-emoji", fragment_id="emoji-fragment", session_id="window-1",
                      scope="technical", expected_revision=1, kind="undo", before="😀 сняли слот памяти", after="😀 сняли свод памяти", target_request_id="emoji")
    redo = replace("redo-slot", "slot-fragment", "сняли свод памяти", "сняли слот памяти", revision=2)
    return dict(initial=requests, expected_initial=expected, undo=[undo, undo_emoji], redo=redo,
                expected_undo=expected + [retract("undo-slot", "slot"), retract("undo-emoji", "emoji")],
                expected_redo=expected + [retract("undo-slot", "slot"), retract("undo-emoji", "emoji"), correction("redo-slot", redo["before"], redo["after"])])
