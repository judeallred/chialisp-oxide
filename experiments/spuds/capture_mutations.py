#!/usr/bin/env python3
"""Own-reveal actual-source policy mutation controls, separate from base captures."""
from pathlib import Path
import sys
BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(BASE.parent/'stateful-ledger'))
from mutation_support import main
if __name__=='__main__':main(BASE)
