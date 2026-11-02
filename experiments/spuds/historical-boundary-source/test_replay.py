"""Vector construction controls; actual VM results are a separate lane."""
import unittest
import replay
import raw_tree


class VectorTests(unittest.TestCase):
    def test_finite_vector_inventory_and_typed_expectations(self):
        rows=replay.vectors()
        self.assertEqual(len(rows),51)
        self.assertEqual(len({row['label'] for row in rows}),51)
        self.assertEqual(sum(row['expected']['status']=='accepted' for row in rows),19)
        self.assertTrue(all(row['failure']==dict(kind='program',code='raise') for row in rows if row['expected']['status']=='rejected'))

    def test_all_accepted_outputs_are_exact_six_condition_proper_lists(self):
        for row in replay.vectors():
            if row['expected']['status']!='accepted':continue
            tree=raw_tree.deserialize(bytes.fromhex(row['expected']['output_hex']))
            conditions=[]
            while type(tree) is tuple:
                conditions.append(tree[0]);tree=tree[1]
            self.assertEqual(tree,b'')
            self.assertEqual(len(conditions),6)
            self.assertEqual([int.from_bytes(condition[0],'big') for condition in conditions],[50,73,82,82 if 'claim' in row['label'] else 86,51,60])

    def test_signature_and_height_enforcement_are_not_vm_rejection_claims(self):
        rows=replay.vectors()
        self.assertTrue(any('invalid BLS key' in row['label'] and row['expected']['status']=='accepted' for row in rows))
        self.assertTrue(all(row['expected']['status']=='accepted' for row in rows if row['label'].startswith('claim round')))


if __name__=='__main__':unittest.main()
