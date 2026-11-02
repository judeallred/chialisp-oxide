#!/usr/bin/env python3
"""Current successful capture; historical boundary evidence remains separate."""
from pathlib import Path
import sys
BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(BASE.parent/'stateful-ledger'))
from capture_support import main
if __name__=='__main__':main(BASE)
