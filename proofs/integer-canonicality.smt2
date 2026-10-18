; Model-level equivalence of sign-extension minimality and two unsigned-byte
; lexicographic bounds, over arbitrary finite byte sequences. This does not
; prove the actual CLVM opcode, lowering, execution cost or compiler preservation.
(set-option :timeout 10000)
(define-sort Bytes () (Seq (_ BitVec 8)))
(declare-const value Bytes)
(define-fun less_than_two ((s Bytes) (a (_ BitVec 8)) (b (_ BitVec 8))) Bool
  (or (= (seq.len s) 0)
      (and (> (seq.len s) 0)
           (or (bvult (seq.nth s 0) a)
               (and (= (seq.nth s 0) a)
                    (or (= (seq.len s) 1)
                        (and (> (seq.len s) 1) (bvult (seq.nth s 1) b))))))))
(define-fun minimal () Bool
  (or (= (seq.len value) 0)
      (and
        (or (not (= (seq.nth value 0) #x00))
            (and (> (seq.len value) 1) (bvuge (seq.nth value 1) #x80)))
        (or (not (= (seq.nth value 0) #xff))
            (= (seq.len value) 1)
            (bvult (seq.nth value 1) #x80)))))
(define-fun interval () Bool
  (or (= (seq.len value) 0)
      (and (not (less_than_two value #x00 #x80))
           (less_than_two value #xff #x80))))
