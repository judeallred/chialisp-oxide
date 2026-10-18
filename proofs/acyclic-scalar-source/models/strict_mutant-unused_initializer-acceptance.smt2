; Nonvacuity
; Explicit acyclic scalar call-frame model; typed domain after ABI.
; Acceptance/output only; no failure-order, cost/resource or compiler-preservation proof.
(set-logic ALL)
(declare-const arg_0_bits (_ BitVec 8))
(define-fun v_0 () Int (bv2int arg_0_bits))
(define-fun arg_0 () Int v_0)
(define-fun v_1 () Bool true)
(assert true)
(assert v_1)
(check-sat)

(reset)
; Counterexample
; Explicit acyclic scalar call-frame model; typed domain after ABI.
; Acceptance/output only; no failure-order, cost/resource or compiler-preservation proof.
(set-logic ALL)
(declare-const arg_0_bits (_ BitVec 8))
(define-fun v_0 () Int (bv2int arg_0_bits))
(define-fun arg_0 () Int v_0)
(define-fun v_1 () Bool true)
(assert true)
(assert (not (= v_1 (< arg_0 100))))
(check-sat)

