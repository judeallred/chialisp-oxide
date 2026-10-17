import Std

/-!
Pure finite-tree algorithm theorems. This model does not import the Rust
frontend, owned IR, CLVM evaluator, or native allocator. Its theorems do not
establish preservation by any of those implementations or compare resources.
-/

namespace TreeTheorems

inductive Tree where
  | atom : List UInt8 → Tree
  | pair : Tree → Tree → Tree
  deriving DecidableEq, Repr

/-- Decode exactly a proper list: arbitrary elements, empty atom terminator. -/
def decodeList : Tree → Option (List Tree)
  | .atom [] => some []
  | .atom (_ :: _) => none
  | .pair first rest => (decodeList rest).map (first :: ·)

/-- The mathematical encoding of a finite proper list. -/
def listTree (items : List Tree) : Tree :=
  items.foldr Tree.pair (.atom [])

/-- Properness refers to the list encoding, rather than algorithm success. -/
def ProperList (tree : Tree) : Prop :=
  ∃ items, tree = listTree items

/-- Direct prefix algorithm: the final tail may be any tree, including an atom
with arbitrary bytes or an improper pair. Only the left input must be proper. -/
def prefixTree : Tree → Tree → Option Tree
  | .atom [], tail => some tail
  | .atom (_ :: _), _ => none
  | .pair first rest, tail => (prefixTree rest tail).map (.pair first)

/-- Independent specification: decode the left list, then fold its elements
onto the supplied tail. It does not call the direct prefix algorithm. -/
def prefixSpec (left tail : Tree) : Option Tree :=
  (decodeList left).map (fun items => items.foldr Tree.pair tail)

theorem decode_listTree (items : List Tree) :
    decodeList (listTree items) = some items := by
  induction items with
  | nil => rfl
  | cons first rest ih =>
    change (decodeList (listTree rest)).map (first :: ·) = some (first :: rest)
    rw [ih]
    rfl

theorem decode_sound (tree : Tree) (items : List Tree)
    (decoded : decodeList tree = some items) : tree = listTree items := by
  induction tree generalizing items with
  | atom bytes =>
    cases bytes with
    | nil =>
      have same : ([] : List Tree) = items := Option.some.inj decoded
      cases same
      rfl
    | cons byte bytes => simp [decodeList] at decoded
  | pair first rest _ ih =>
    cases decodedRest : decodeList rest with
    | none => simp [decodeList, decodedRest] at decoded
    | some restItems =>
      have same : first :: restItems = items := by
        simpa [decodeList, decodedRest] using decoded
      cases same
      exact congrArg (Tree.pair first) (ih restItems decodedRest)

theorem decode_eq_some_iff (tree : Tree) (items : List Tree) :
    decodeList tree = some items ↔ tree = listTree items := by
  constructor
  · exact decode_sound tree items
  · intro encoded
    rw [encoded, decode_listTree]

theorem decode_none_iff_not_proper (tree : Tree) :
    decodeList tree = none ↔ ¬ ProperList tree := by
  constructor
  · intro failed ⟨items, encoded⟩
    have decoded := (decode_eq_some_iff tree items).mpr encoded
    rw [failed] at decoded
    cases decoded
  · intro improper
    cases decoded : decodeList tree with
    | none => rfl
    | some items =>
      exact False.elim (improper ⟨items, decode_sound tree items decoded⟩)

/-- Extensional equality for all finite trees and every arbitrary final tail. -/
theorem prefix_correct (left tail : Tree) :
    prefixTree left tail = prefixSpec left tail := by
  induction left with
  | atom bytes => cases bytes <;> rfl
  | pair first rest _ ih =>
    simp only [prefixTree, prefixSpec, decodeList] at ih ⊢
    rw [ih]
    cases decodeList rest <;> rfl

theorem prefix_listTree (items : List Tree) (tail : Tree) :
    prefixTree (listTree items) tail = some (items.foldr Tree.pair tail) := by
  rw [prefix_correct]
  unfold prefixSpec
  rw [decode_listTree]
  rfl

theorem prefix_none_iff_decode_none (left tail : Tree) :
    prefixTree left tail = none ↔ decodeList left = none := by
  rw [prefix_correct]
  unfold prefixSpec
  cases decodeList left <;> simp

/-- Failure depends only on improperness of the left input, not tail shape. -/
theorem prefix_failure_iff (left tail : Tree) :
    prefixTree left tail = none ↔ ¬ ProperList left := by
  exact (prefix_none_iff_decode_none left tail).trans (decode_none_iff_not_proper left)

theorem prefix_success_iff (left tail output : Tree) :
    prefixTree left tail = some output ↔
      ∃ items, left = listTree items ∧ output = items.foldr Tree.pair tail := by
  constructor
  · intro succeeded
    rw [prefix_correct] at succeeded
    unfold prefixSpec at succeeded
    cases decoded : decodeList left with
    | none => simp [decoded] at succeeded
    | some items =>
      have same : items.foldr Tree.pair tail = output := by
        simpa [decoded] using succeeded
      exact ⟨items, decode_sound left items decoded, same.symm⟩
  · intro ⟨items, encoded, outputEq⟩
    rw [encoded, outputEq]
    exact prefix_listTree items tail

/-- Follow exactly n right links, retaining the entire final tree. -/
def dropRight : Nat → Tree → Option Tree
  | 0, tree => some tree
  | _ + 1, .atom _ => none
  | n + 1, .pair _ rest => dropRight n rest

theorem foldr_preserves_tail (items : List Tree) (tail : Tree) :
    dropRight items.length (items.foldr Tree.pair tail) = some tail := by
  induction items with
  | nil => rfl
  | cons first rest ih => exact ih

/-- Successful prefixing preserves the complete arbitrary supplied tail at
the exact boundary, not merely its truthiness or numeric atom value. -/
theorem prefix_preserves_tail (left tail output : Tree)
    (succeeded : prefixTree left tail = some output) :
    ∃ items, left = listTree items ∧ dropRight items.length output = some tail := by
  obtain ⟨items, encoded, outputEq⟩ := (prefix_success_iff left tail output).mp succeeded
  exact ⟨items, encoded, outputEq ▸ foldr_preserves_tail items tail⟩

/-- Direct model of the published last_inner helper's structural behavior:
destructure a pair, return its first element at nil, otherwise follow rest. -/
def lastInner (tree : Tree) : Option Tree :=
  match tree with
  | .atom _ => none
  | .pair first rest =>
    match rest with
    | .atom [] => some first
    | .atom (_ :: _) => none
    | .pair _ _ => lastInner rest
termination_by structural tree

/-- Independent list-library specification; the empty list has no last item. -/
def lastSpec (tree : Tree) : Option Tree :=
  (decodeList tree).bind List.getLast?

theorem lastInner_correct (tree : Tree) : lastInner tree = lastSpec tree := by
  induction tree with
  | atom bytes => cases bytes <;> rfl
  | pair first rest _ ih =>
    cases rest with
    | atom bytes => cases bytes <;> rfl
    | pair next ending =>
      change lastInner (.pair next ending) = lastSpec (.pair first (.pair next ending))
      rw [ih]
      unfold lastSpec
      cases decoded : decodeList ending <;> simp [decodeList, decoded]

theorem lastInner_listTree (items : List Tree) :
    lastInner (listTree items) = items.getLast? := by
  rw [lastInner_correct]
  unfold lastSpec
  rw [decode_listTree]
  rfl

theorem lastInner_rejects_atom (bytes : List UInt8) :
    lastInner (.atom bytes) = none := by
  rfl

theorem lastInner_rejects_nonempty_tail
    (first : Tree) (byte : UInt8) (bytes : List UInt8) :
    lastInner (.pair first (.atom (byte :: bytes))) = none := by
  rfl

/-- Separate external ABI model: exactly two arguments and a literal empty
atom terminator. The argument subtrees themselves are unrestricted. -/
def strictTwo : Tree → Option (Tree × Tree)
  | .pair first (.pair second (.atom [])) => some (first, second)
  | _ => none

theorem strictTwo_exact (environment first second : Tree) :
    strictTwo environment = some (first, second) ↔
      environment = listTree [first, second] := by
  cases environment with
  | atom bytes => simp [strictTwo, listTree]
  | pair head rest =>
    cases rest with
    | atom bytes => simp [strictTwo, listTree]
    | pair next ending =>
      cases ending with
      | pair firstTail restTail => simp [strictTwo, listTree]
      | atom bytes => cases bytes <;> simp [strictTwo, listTree]

theorem strictTwo_rejects_extra (first second third rest : Tree) :
    strictTwo (.pair first (.pair second (.pair third rest))) = none := by
  rfl

theorem strictTwo_rejects_nonempty_terminator
    (first second : Tree) (byte : UInt8) (bytes : List UInt8) :
    strictTwo (.pair first (.pair second (.atom (byte :: bytes)))) = none := by
  rfl

/-- Compose ABI validation with the independently specified algorithm. -/
def prefixEntry (environment : Tree) : Option Tree :=
  (strictTwo environment).bind (fun arguments => prefixTree arguments.1 arguments.2)

theorem prefixEntry_exact (left tail : Tree) :
    prefixEntry (listTree [left, tail]) = prefixTree left tail := by
  rfl

theorem prefixEntry_rejects_extra (first second third rest : Tree) :
    prefixEntry (.pair first (.pair second (.pair third rest))) = none := by
  rfl

theorem prefixEntry_rejects_nonempty_terminator
    (first second : Tree) (byte : UInt8) (bytes : List UInt8) :
    prefixEntry (.pair first (.pair second (.atom (byte :: bytes)))) = none := by
  rfl

-- Audit every public theorem, including intermediate lemmas. No proof uses
-- native evaluation; all recursion/induction and equality checks reach Lean's
-- kernel. Inspect this output together with the actual theorem statements.
#print axioms decode_listTree
#print axioms decode_sound
#print axioms decode_eq_some_iff
#print axioms decode_none_iff_not_proper
#print axioms prefix_correct
#print axioms prefix_listTree
#print axioms prefix_none_iff_decode_none
#print axioms prefix_failure_iff
#print axioms prefix_success_iff
#print axioms foldr_preserves_tail
#print axioms prefix_preserves_tail
#print axioms lastInner_correct
#print axioms lastInner_listTree
#print axioms lastInner_rejects_atom
#print axioms lastInner_rejects_nonempty_tail
#print axioms strictTwo_exact
#print axioms strictTwo_rejects_extra
#print axioms strictTwo_rejects_nonempty_terminator
#print axioms prefixEntry_exact
#print axioms prefixEntry_rejects_extra
#print axioms prefixEntry_rejects_nonempty_terminator

end TreeTheorems
