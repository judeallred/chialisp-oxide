"""Pure companion-file admission regressions; no capture or execution credit."""
import copy
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import action_counter_packet as packet


class CompanionTests(unittest.TestCase):
    def test_producer_uses_canonical_python(self):
        protocol=packet.protocol_capture
        args=SimpleNamespace(**{name:Path('/fixture')/name for name in
            ('output','native_target','compiler_target','compiler_root','rustc','driver','cargo_cli')})
        with patch.object(protocol.sys,'executable','/fixture/alias/python'), \
             patch.object(Path,'resolve',return_value=Path('/fixture/canonical/python')), \
             patch.object(protocol.support,'command',return_value=b'') as command:
            protocol.raw_read(args,Path('/fixture/result'),'raw-schema2-read')
        self.assertEqual(command.call_args.args[2][0],Path('/fixture/canonical/python'))

    def test_production_rejects_alias_and_command_tampering(self):
        args=SimpleNamespace(rustc=Path('/fixture/rustc'))
        target=Path('/fixture/target');argv=['/fixture/canonical/python','/fixture/capture.py','read']
        base={'argv':argv,'cwd':str(packet.ROOT),'target':str(target),'jobs':2,'offline':True,
              'rustc':str(args.rustc),'driver':None,'exit_code':0,'status':'passed',
              'failure_kind':None,'failure_detail':None,'timeout_seconds':packet.support.COMMAND_TIMEOUT,
              'cleanup_timeout_seconds':packet.support.COMMAND_CLEANUP_TIMEOUT,
              'max_stream_bytes':packet.support.MAX_FILE,'stdout':{},'stderr':{}}
        class Commands:
            index={'origin_root':str(packet.ROOT)}
            def load(self,name):return self.receipt
        commands=Commands()
        for changed in (['/fixture/alias/python',*argv[1:]],
                        [argv[0],'/fixture/tampered.py',argv[2]],
                        [*argv[:2],'tampered']):
            with self.subTest(argv=changed):
                receipt=copy.deepcopy(base);receipt['argv']=changed;commands.receipt=receipt
                with self.assertRaises(ValueError):
                    packet.support.check_receipt(receipt,argv,target=target,rustc=args.rustc)
                with self.assertRaises(ValueError):
                    packet.check_command(commands,'raw-schema2-read',argv,target,args,'protocol')

    def fixture(self):
        name='fixture::unselected';stem=packet.digest(name.encode())
        entry={'entry':name,'ir':stem+'.ir.json','clvm':stem+'.clvm.hex',
               'artifact':{'backend':'rust_mir','optimize_lir':False,'bytes':1,
                           'clvm_hex':'80','tree_hash':'00'*32,'disassembly':'()'}}
        class Payloads:
            def load(self,path):
                if path not in self.data:raise ValueError('missing companion')
                return copy.deepcopy(self.data[path])
            def raw(self,path):
                if path not in self.data:raise ValueError('missing companion')
                return self.data[path]
        payloads=Payloads();payloads.data={'base/'+stem+'.artifact.json':copy.deepcopy(entry['artifact']),
                                         'base/'+stem+'.clvm.txt':b'()\n'}
        return entry,payloads,stem

    def test_complete_typed_companions(self):
        entry,payloads,stem=self.fixture()
        self.assertEqual(packet.admit_companions(payloads,'base',entry,'off'),
                         {stem+'.artifact.json',stem+'.clvm.txt'})

    def test_companion_corruptions(self):
        for kind in ('missing-artifact','missing-text','unknown-field','float-bytes',
                     'boolean-bytes','wrong-mode','changed-artifact','no-newline',
                     'extra-newline','wrong-stem','wrong-clvm-stem'):
            with self.subTest(kind=kind):
                entry,payloads,stem=self.fixture();a='base/'+stem+'.artifact.json';t='base/'+stem+'.clvm.txt'
                if kind=='missing-artifact':del payloads.data[a]
                elif kind=='missing-text':del payloads.data[t]
                elif kind=='unknown-field':payloads.data[a]['unknown']=True
                elif kind=='float-bytes':payloads.data[a]['bytes']=1.0
                elif kind=='boolean-bytes':payloads.data[a]['bytes']=True
                elif kind=='wrong-mode':payloads.data[a]['optimize_lir']=True
                elif kind=='changed-artifact':payloads.data[a]['disassembly']='changed'
                elif kind=='no-newline':payloads.data[t]=b'()'
                elif kind=='extra-newline':payloads.data[t]=b'()\n\n'
                elif kind=='wrong-stem':entry['ir']='wrong.ir.json'
                elif kind=='wrong-clvm-stem':entry['clvm']='wrong.clvm.hex'
                with self.assertRaises(ValueError):packet.admit_companions(payloads,'base',entry,'off')


if __name__=='__main__':unittest.main()
