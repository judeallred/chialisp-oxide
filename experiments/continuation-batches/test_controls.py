"""Reader/recipe controls; no VM, Cargo or SDK execution by these tests."""
from pathlib import Path
import sys
import unittest
import oracle
import replay
import mutation_cases
BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(BASE.parent/'stateful-ledger'))
from mutation_support import check_recipe,expected
from historical_boundary import validate

class CurrentControls(unittest.TestCase):
    def test_exact_source_mutations_and_independent_witness_inventory(self):
        source=(BASE/'src/lib.rs').read_text();cases=mutation_cases.witnesses(oracle)
        self.assertEqual({w['name'] for w in cases},set(mutation_cases.RECIPES))
        self.assertEqual(len(cases),4 if BASE.name=='continuation-batches' else 5)
        for name,(old,new) in mutation_cases.RECIPES.items():
            self.assertEqual(source.count(old),1);self.assertNotEqual(old,new)
            check_recipe(dict(before=old,after=new,occurrences=1),old,new)
            with self.assertRaises(ValueError):check_recipe(dict(before=old,after=old,occurrences=1),old,new)
        self.assertTrue(all(expected(w['mutant_conditions'])['status']=='accepted' for w in cases))
    def test_incomplete_vm_mode_inventory_fails_before_output_creation(self):
        with self.assertRaises(ValueError):replay.execute(Path('/unused'),dict(rust_on=Path('/unused')),Path('/unused'))
    def test_strict_extra_tail_failure_modes_are_explicit(self):
        rows=[r for r in replay.vectors() if r.get('failure_by_mode')]
        self.assertEqual(len(rows),5 if BASE.name=='continuation-batches' else 2)
        self.assertTrue(all(r['failure_by_mode']=={'rust_off':dict(kind='program',code='invalid_op_arg')} for r in rows))
    def test_old_boundary_is_bound_to_frozen_historical_source(self):
        r=validate(BASE);self.assertFalse(r['current_source_freshness']);self.assertFalse(r['current_compiler_boundary_credit']);self.assertFalse(r['current_success_credit'])

if __name__=='__main__':unittest.main()
