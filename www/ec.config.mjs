import { chialispGrammar, customizeChialispTheme } from './chialisp-highlighting.mjs';

// A shared config file lets both Markdown rendering and Starlight's <Code>
// component load the actual theme callback, rather than JSON-serializing it.
export default {
  shiki: { langs: [chialispGrammar] },
  customizeTheme: customizeChialispTheme,
  defaultProps: {
    overridesByLang: { rust: { wrap: true, preserveIndent: true } },
  },
};
