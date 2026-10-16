Next phase of the Rust → CLVM compiler project. Goal: prove out and validate the compiler tooling, and end up with a comprehensive tutorial and demonstration system.

## Tutorials
- Use [chialisp.com](https://chialisp.com/) as a primary reference for tutorials, language and CLVM behavior, examples, costs, conditions, and documented pitfalls.
- Draw on all the existing Chialisp tutorials (coins, puzzles, etc.) and reimplement each one in Rust using our compiler.
- Show Chialisp, Rust, and Rue source side by side, with actual independently compiled artifacts, outcomes, costs, and differences. Distinguish Rust using Rue's backend from compiling Rue source.
- Show differences that come from going through the Rust compiler.
- Showcase new things that are possible because we're in Rust (e.g. Rust libraries we can use).

## Error study
- Detailed study of the errors you can make writing pure Chialisp or Rue that become impossible with our Rust → CLVM compiler.
- Concise list of "errors now impossible" on the main page, linking to the full detailed study.

## Puzzle reimplementations
- Reimplement every known Chialisp puzzle in any of the CHIPs.
- For each: the original and the Rust implementation side by side, plus a diff of the compiled output.

Related: [Code] xch.nyc: Rust → Rue compiler prototype (https://trello.com/c/zsnMy0l7)
Part of [Code] xch.nyc (https://trello.com/c/JJAi4Vdo)

## Puzzle testability and security confidence
- Make native execution, differential CLVM tests, malformed-input tests, property-based tests, and reproducible cost reports part of the puzzle authoring workflow.
- Validate complete coin spends in a simulator, including signatures, announcements, timelocks, and adversarial compositions.
- Explore Lean proofs and SMT/model checking for explicit authorization, conservation, state-transition, and compiler-preservation properties.
- State each claim's assumptions, checked scope, and remaining gaps. Passing tests or proving an abstract model must not be presented as proof that a puzzle has no security breaches.
- Include mutation controls: deliberately remove security checks and require the verification system to detect the resulting counterexample.

## Published sources, parity, and teaching quality
- Discover and catalog published puzzles in Chialisp, Rue, and Rust across official and community repositories; pin provenance, source files, licenses, and the scope searched. Do not equate a Rust host-side puzzle builder with a Rust puzzle compiled by this project.
- Demonstrate the Chialisp functionality available through Rust with executable witnesses. Publish missing features, semantic differences, and features that should require explicit APIs. A roadmap or general Rust expressibility argument is not a compiler-preservation proof.
- Build a dedicated “Errors that Rust makes impossible” section. For each precise claim, execute a Chialisp bug that passes an identified limited test suite, show its missed counterexample, and capture the Rust compiler/checker rejection. Distinguish compiler guarantees from runtime ABI rejection, model checking, and policy mistakes that remain possible; report Rue's existing protections accurately.
- Make this an understandable route to Chia coin programming: progressive tutorials, three-language comparisons, reproducible security harnesses, source discovery, searchable references, and honest readiness and coverage limits. Continue improving implementation, teaching, tests, and documentation together.
