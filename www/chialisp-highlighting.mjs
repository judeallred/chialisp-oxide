// Chialisp-specific TextMate adaptation, changed from the official Prism design.
// Reference: Chia-Network/chialisp-web@dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8
// Copyright 2026 Chia Network Inc. Apache-2.0; attribution/license in public/licenses.
// This port adds modern forms/operators, exact token boundaries and contrast
// adjustments. It is presentation only, not a Chialisp parser or validator.
const boundary = '(?=$|[\\s()\\[\\]])';
const primitiveStart = '(?<![^\\s()\\[\\]])';
const head = words => `(?<=\\()\\s*(?:${words})${boundary}`;
export const chialispGrammar = {
  name: 'clsp', aliases: ['chialisp'], displayName: 'Chialisp', scopeName: 'source.chialisp',
  patterns: [
    { name: 'comment.line.semicolon.chialisp', begin: ';', end: '$' },
    { name: 'string.quoted.double.chialisp', begin: '"', end: '"', patterns: [
      { name: 'constant.character.escape.chialisp', match: '\\\\.' },
    ] },
    { name: 'constant.numeric.hex.chialisp', match: `${primitiveStart}0x[0-9a-fA-F]+${boundary}` },
    { name: 'constant.numeric.integer.chialisp', match: `${primitiveStart}[-+]?\\d+${boundary}` },
    { match: '(?<=\\()\\s*(defun-inline|defun|defmacro)(\\s+)([^\\s()]+)', captures: {
      1: { name: 'keyword.control.chialisp' }, 3: { name: 'entity.name.function.chialisp' },
    } },
    { name: 'keyword.control.chialisp', match: head('mod|defconstant|include|lambda|if|i|let\\*?|assign') },
    { name: 'support.function.list.chialisp', match: head('q|qq|unquote|f|r|c|l|list') },
    { name: 'support.function.builtin.chialisp', match: head('sha256|x|a|=|>|>s|concat|\\+|-|/|\\*|divmod|logand|logior|logxor|lognot|ash|lsh|substr|strlen|point_add|pubkey_for_exp|not|any|all|softfork|coinid|modpow') },
    { name: 'constant.other.chialisp', match: `\\b[A-Z][A-Z0-9_]*${boundary}` },
    { name: 'entity.name.function.chialisp', match: '(?<=\\()\\s*[^\\s()";]+' },
    { name: 'punctuation.section.chialisp', match: '[()\\[\\]]|(?<=\\s)\\.(?=\\s)' },
  ],
};

// Preserve the official purple/green/cyan/orange distinction. Comments are
// brighter in dark mode; the upstream dark comment color fails 4.5:1 contrast.
export const palettes = {
  dark: { keyword: '#bd93f9', list: '#50fa7b', builtin: '#05e3df', literal: '#ffb86c', comment: '#a5b3d5', function: '#f1fa8c', plain: '#f8f8f2' },
  light: { keyword: '#990096', list: '#006100', builtin: '#127eaf', literal: '#b35c00', comment: '#64646e', function: '#0045db', plain: '#383a42' },
};
export function chialispThemeRules(type) {
  const p = palettes[type];
  return [
    ['source.chialisp', p.plain], ['keyword.control.chialisp', p.keyword],
    ['support.function.list.chialisp', p.list], ['support.function.builtin.chialisp', p.builtin],
    ['constant.numeric.hex.chialisp, constant.numeric.integer.chialisp, constant.other.chialisp, string.quoted.double.chialisp, constant.character.escape.chialisp', p.literal],
    ['comment.line.semicolon.chialisp', p.comment], ['entity.name.function.chialisp', p.function],
    ['punctuation.section.chialisp', p.plain],
  ].map(([scope, foreground]) => ({ scope: scope.split(', '), settings: { foreground } }));
}
export function customizeChialispTheme(theme) {
  theme.settings.push(...chialispThemeRules(theme.type));
  // Expressive Code applies its existing minimum contrast adjustment after
  // customizeTheme, against the actual configured code background.
}
