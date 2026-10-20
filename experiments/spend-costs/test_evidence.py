"""Constructed corruption controls, separate from VM/BLS/budget execution."""
import copy
import unittest
import check as evidence


class EvidenceControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report_bytes = (evidence.ROOT / "corpus/validation.json").read_bytes()
        cls.inputs = evidence.jobs(evidence.load(evidence.ROOT / "corpus/validation.json"))
        cls.record = evidence.load(evidence.HERE / "results.json")

    def check(self, value):
        return evidence.validate(value, self.report_bytes, self.inputs)

    def reject(self, change):
        value = copy.deepcopy(self.record)
        change(value)
        with self.assertRaises((ValueError, KeyError, TypeError)):
            self.check(value)

    def test_complete_record_and_independent_generators(self):
        self.assertGreater(self.check(self.record)["bundles"], 0)

    def test_identity_scope_and_source_corruption(self):
        changes = [
            lambda r: r.update(scope="production security proof"),
            lambda r: r.update(validation_report_sha256="0"*64),
            lambda r: r.update(input_sha256="0"*64),
            lambda r: r["source_window"].pop(),
            lambda r: r["source_window"][0].update(sha256="0"*64),
            lambda r: r["measurement"]["cases"].pop(),
            lambda r: r["measurement"]["cases"].reverse(),
            lambda r: r["measurement"]["cases"][0].update(optimize_lir=0),
            lambda r: r["measurement"]["cases"][0].update(label="other input"),
        ]
        for change in changes:
            with self.subTest(change=changes.index(change)): self.reject(change)

    def test_metric_and_boundary_corruption(self):
        changes = [
            lambda c: c.update(flags=c["flags"] | 0x1_0000),
            lambda c: c.update(previous_transaction_height=1),
            lambda c: c.update(max_cost=c["max_cost"]+1),
            lambda c: c.update(cost_per_byte=1),
            lambda c: c.update(total_reveal_bytes=c["total_reveal_bytes"]+1),
            lambda c: c["helper"].update(signature_verified=True),
            lambda c: c["helper"].update(execution_cost=float(c["helper"]["execution_cost"])),
            lambda c: c["helper"].update(charged_bytes=c["helper"]["charged_bytes"]+2),
            lambda c: c["helper"].update(total_cost=c["helper"]["total_cost"]+1),
            lambda c: c["helper"]["per_spend"][0].update(coin_id="0"*64),
            lambda c: c["generators"].reverse(),
            lambda c: c["generators"][0].update(generator_hex=c["generators"][0]["generator_hex"]+"80"),
            lambda c: c["generators"][1].update(generator_bytes=True),
            lambda c: c["generators"][1].update(generator_hex="00" + c["generators"][1]["generator_hex"][2:]),
            lambda c: c["generators"][1]["metrics"].update(signature_verified=1),
            lambda c: c["generators"][1]["signature_controls"].pop(),
            lambda c: c["generators"][0]["signature_controls"][0].update(code="CostExceeded"),
            lambda c: c["generators"][0]["budget_control"].update(one_less_code="GeneratorRuntimeError"),
            lambda c: c["generators"][1]["budget_control"].update(exact_cost_succeeded=1),
        ]
        for index, change in enumerate(changes):
            with self.subTest(change=index):
                self.reject(lambda r, change=change: change(r["measurement"]["cases"][0]["costs"]))


if __name__ == "__main__":
    unittest.main()
