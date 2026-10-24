#!/usr/bin/env python3
"""Check original counter provenance and portable captured/source-control data."""
import sys
sys.dont_write_bytecode = True
import argparse
import json
from pathlib import Path

import action_counter_packet as packet
from action_counter_packet_controls import self_test, structural_controls


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet',type=Path,default=packet.ROOT/'corpus/action-counter-source-controls')
    parser.add_argument('--source-only',action='store_true')
    parser.add_argument('--self-test',action='store_true')
    parser.add_argument('--replay',action='store_true')
    parser.add_argument('--cli',type=Path,default=packet.ROOT/'target/debug/chialisp-oxide')
    parser.add_argument('--install',type=Path)
    parser.add_argument('--raw-capture',type=Path)
    parser.add_argument('--protocol-capture',type=Path)
    args=parser.parse_args()
    result={'provenance':packet.check_provenance()}
    if args.install:
        if not (args.raw_capture and args.protocol_capture):
            parser.error('--raw-capture and --protocol-capture are required with --install')
        result['installed']=packet.install(args.raw_capture,args.protocol_capture,args.install)
        args.packet=args.install
    if args.source_only:
        if args.replay or args.install:
            parser.error('--source-only cannot include --replay or --install')
        result['scope']='Source identity only; no captured execution credit.'
        if args.self_test:result['constructed_profile_controls']=structural_controls()
    else:
        result['packet']=packet.validate(args.packet)
        if args.self_test:result['corruption_controls']=self_test(args.packet)
        if args.replay:
            from action_counter_replay import replay
            result['replay']=replay(args.packet,args.cli)
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()
