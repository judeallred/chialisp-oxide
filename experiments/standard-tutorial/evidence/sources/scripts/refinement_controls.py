"""Direct nonvacuous controls for the signed c879 refinement boundary."""
from inline_controls import self_test as shared_controls
from runtime_refinement_migration import predecessor, preserve, project_validated


def self_test(report, *, current_validation=preserve):
    return shared_controls(report, projection=project_validated,
        current_validation=current_validation, normalize_refinement=False,
        baseline_ledger=predecessor()["reviewed_after"]["puzzles"][-1]["curried_instances"]["ledger"])
