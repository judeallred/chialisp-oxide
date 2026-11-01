"""Vector construction controls; no VM execution by these reader tests."""
import unittest
import replay


class VectorControls(unittest.TestCase):
    def test_complete_unique_parseable_raw_environments(self):
        rows=replay.vectors()
        self.assertEqual(len(rows),59)
        self.assertEqual(len({row['label'] for row in rows}),59)
        for row in rows:
            replay.raw_tree.parse(row['solution'])
        self.assertEqual(sum(row['expected']['status']=='accepted' for row in rows),11)

    def test_positive_outputs_are_actual_serialized_condition_trees(self):
        for row in replay.vectors():
            if row['expected']['status']=='accepted':
                blob=bytes.fromhex(row['expected']['output_hex'])
                self.assertEqual(replay.raw_tree.serialize(replay.raw_tree.deserialize(blob)),blob)

    def test_typed_abi_failure_divergences_remain_explicit(self):
        rows=replay.vectors()
        divergent=[row['label'] for row in rows if row.get('failure_by_mode')]
        self.assertEqual(len(divergent),5)
        for row in rows:
            if row['expected']['status']=='rejected':
                self.assertEqual(row['failure'],dict(kind='program',code='raise'))


if __name__=='__main__':unittest.main()
