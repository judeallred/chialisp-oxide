#!/usr/bin/env python3
"""Read-only historical boundary check; no current compiler-boundary claim."""
from pathlib import Path
import sys,json
BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(BASE.parent/'stateful-ledger'))
from historical_boundary import validate
if __name__=='__main__':print(json.dumps(validate(BASE),indent=2))
