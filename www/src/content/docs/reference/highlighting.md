---
title: Read Chialisp syntax colors
description: Chialisp-specific token categories in the source comparisons, with pinned official provenance and accessible theme colors.
---

Chialisp snippets use a registered `clsp` language, also available as `chialisp`.
The grammar runs through the site's existing Shiki/Expressive Code renderer.

The color families follow the
[official Chialisp Prism grammar and themes](https://github.com/Chia-Network/chialisp-web/tree/dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8/src/theme):

- Purple: module, definition and control forms, including `mod`, `defun`,
  `defun-inline`, `defmacro`, `defconstant`, `include`, `lambda` and `if`.
- Green: quoting and list operations, such as `q`, `qq`, `unquote`, `f`, `r`,
  `c`, `l` and `list`.
- Cyan: CLVM builtins, including arithmetic, `a`, `x`, `sha256`, byte operations
  and public-key operations.
- Orange: integer and hexadecimal atoms, strings and uppercase symbols.
- Yellow in dark mode, blue in light mode: ordinary calls and defined function names.
- Muted text: semicolon comments; a semicolon inside a quoted string stays a string.

The port adds modern forms, uppercase symbols and exact literal boundaries;
it does not copy the Prism parser wholesale or attempt lexical binding analysis.
An uppercase parameter can share a color with a constant; color does not resolve
the binding.
Long compiled disassemblies and serialized hex use clearly labeled prefixes in
the laboratory, with exact complete downloads. The three source-language
comparisons remain complete. This keeps syntax spans within the page-size
budgets without hiding the full program artifacts.
The upstream dark comment color is brightened, and Expressive Code adjusts
colors against the site's actual code backgrounds. The build checks rendered
token contrast in both themes, source preservation, aliases and token boundaries.
Coloring is a reading aid, not parsing, type checking or evidence of validity.

Source is pinned to `dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8`, under
Apache-2.0. The upstream theme credits Dracula as its palette origin. See the
[attribution](/chialisp-oxide/licenses/chialisp-highlighting-notice.txt) and
[license](/chialisp-oxide/licenses/chialisp-highlighting-apache-2.0.txt).
