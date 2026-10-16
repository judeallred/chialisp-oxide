; Model-level checks only: there is not yet a proof linking this to Rust/CLVM.
(set-logic QF_LIA)
(set-option :timeout 5000)
(declare-const amount Int)
(define-fun spec () Bool (and (<= 0 amount) (<= amount 1000)))
(define-fun lowered () Bool (ite (> amount (- 1)) (> 1001 amount) false))
; No unbounded integer distinguishes these two acceptance predicates.
(push)
(assert (not (= spec lowered)))
(check-sat)
(pop)
; No accepted amount violates the range contract.
(push)
(assert lowered)
(assert (or (< amount 0) (> amount 1000)))
(check-sat)
(pop)
; Mutation control: dropping the upper-bound guard must yield a counterexample.
(push)
(assert (> amount (- 1)))
(assert (not spec))
(check-sat)
(get-value (amount))
(pop)
