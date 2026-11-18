"""Strict bounded decoding of the active complete corpus report.

`corpus/validation.json` outgrew the shared 16 MiB Cargo JSON bound when the
49th export landed. Readers of that one file use this decoder, with its own
explicit bound, rather than widening the shared Cargo/packet decoders that
other retained evidence pins. Duplicate keys (including escaped aliases),
non-finite numbers and oversized payloads reject. The report is produced
locally by the validator; this is a resource bound, not authentication.
"""
import json

REPORT_LIMIT = 32 * 1024 * 1024


def _reject_constant(name):
    raise ValueError(f"non-finite JSON number {name} in corpus report")


def _unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key!r} in corpus report")
        result[key] = value
    return result


def decode(payload):
    if type(payload) is not bytes or not 0 < len(payload) <= REPORT_LIMIT:
        raise ValueError("corpus report byte bound")
    return json.loads(payload.decode("utf-8"), object_pairs_hook=_unique, parse_constant=_reject_constant)
