"""Bounded compiled-artifact codec; separate from the narrow source fixtures.

The retained Chialisp fixture decoder remains unchanged. This consumer profile
allows 1MiB/65,536nodes/512depth and charges at most16MiB of modeled codec work
before building a decoded tree. These are host transport limits, not policy or
compiler guards, execution authentication, or exact Python heap measurements.
"""
from pathlib import Path
import sys
sys.dont_write_bytecode = True
sys.path.insert(0,str(Path(__file__).resolve().parents[2]/"scripts"))
from registry_codegen_evidence import inspect_compiled_artifact, require
MAX_BYTES=1024*1024
MAX_NODES=65536
MAX_DEPTH=512
MAX_WORK=16*1024*1024


def inspect(raw):
    record=inspect_compiled_artifact(raw,max_bytes=MAX_BYTES,max_nodes=MAX_NODES,max_depth=MAX_DEPTH)
    stats=record["stats"]
    charge=stats["nodes"]*192+stats["atom_payload_bytes"]+len(raw)
    require(charge<=MAX_WORK,"compiled decoded allocation/work charge exceeded")
    record["decoded_work_profile"]={"charged_bytes":charge,"maximum":MAX_WORK,"node_charge":192,"scope":"Conservative abstract allocation/work units; not Python heap measurement."}
    return record

