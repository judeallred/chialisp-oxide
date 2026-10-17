import TreeTheorems

/-!
Independent expected types for the 21 claimed public theorems. In particular,
the algorithm specifications are expanded here instead of accepting whichever
type a theorem happens to print. This is a type/coverage oracle, not a second
implementation of Lean's kernel or a compiler-preservation proof.
-/
open TreeTheorems

example : ∀ items : List Tree,
    decodeList (listTree items) = some items :=
  @decode_listTree

example : ∀ (tree : Tree) (items : List Tree),
    decodeList tree = some items → tree = listTree items :=
  @decode_sound

example : ∀ (tree : Tree) (items : List Tree),
    decodeList tree = some items ↔ tree = listTree items :=
  @decode_eq_some_iff

example : ∀ tree : Tree,
    decodeList tree = none ↔ ¬ ∃ items, tree = listTree items :=
  @decode_none_iff_not_proper

example : ∀ left tail : Tree,
    prefixTree left tail = (decodeList left).map (fun items => items.foldr Tree.pair tail) :=
  @prefix_correct

example : ∀ (items : List Tree) (tail : Tree),
    prefixTree (listTree items) tail = some (items.foldr Tree.pair tail) :=
  @prefix_listTree

example : ∀ left tail : Tree,
    prefixTree left tail = none ↔ decodeList left = none :=
  @prefix_none_iff_decode_none

example : ∀ left tail : Tree,
    prefixTree left tail = none ↔ ¬ ∃ items, left = listTree items :=
  @prefix_failure_iff

example : ∀ left tail output : Tree,
    prefixTree left tail = some output ↔
      ∃ items, left = listTree items ∧ output = items.foldr Tree.pair tail :=
  @prefix_success_iff

example : ∀ (items : List Tree) (tail : Tree),
    dropRight items.length (items.foldr Tree.pair tail) = some tail :=
  @foldr_preserves_tail

example : ∀ left tail output : Tree,
    prefixTree left tail = some output →
      ∃ items, left = listTree items ∧ dropRight items.length output = some tail :=
  @prefix_preserves_tail

example : ∀ tree : Tree,
    lastInner tree = (decodeList tree).bind List.getLast? :=
  @lastInner_correct

example : ∀ items : List Tree,
    lastInner (listTree items) = items.getLast? :=
  @lastInner_listTree

example : ∀ bytes : List UInt8,
    lastInner (.atom bytes) = none :=
  @lastInner_rejects_atom

example : ∀ (first : Tree) (byte : UInt8) (bytes : List UInt8),
    lastInner (.pair first (.atom (byte :: bytes))) = none :=
  @lastInner_rejects_nonempty_tail

example : ∀ environment first second : Tree,
    strictTwo environment = some (first, second) ↔ environment = listTree [first, second] :=
  @strictTwo_exact

example : ∀ first second third rest : Tree,
    strictTwo (.pair first (.pair second (.pair third rest))) = none :=
  @strictTwo_rejects_extra

example : ∀ (first second : Tree) (byte : UInt8) (bytes : List UInt8),
    strictTwo (.pair first (.pair second (.atom (byte :: bytes)))) = none :=
  @strictTwo_rejects_nonempty_terminator

example : ∀ left tail : Tree,
    prefixEntry (listTree [left, tail]) = prefixTree left tail :=
  @prefixEntry_exact

example : ∀ first second third rest : Tree,
    prefixEntry (.pair first (.pair second (.pair third rest))) = none :=
  @prefixEntry_rejects_extra

example : ∀ (first second : Tree) (byte : UInt8) (bytes : List UInt8),
    prefixEntry (.pair first (.pair second (.atom (byte :: bytes)))) = none :=
  @prefixEntry_rejects_nonempty_terminator
