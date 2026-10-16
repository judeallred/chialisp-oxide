Next phase of the Rust → CLVM compiler project. Goal: prove out and validate the compiler tooling, and end up with a comprehensive tutorial and demonstration system.

## Tutorials
- Use [chialisp.com](https://chialisp.com/) as a primary reference for tutorials, language and CLVM behavior, examples, costs, conditions, and documented pitfalls.
- Draw on all the existing Chialisp tutorials (coins, puzzles, etc.) and reimplement each one in Rust using our compiler.
- Show direct side-by-side comparisons between the Chialisp and Rust versions.
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
