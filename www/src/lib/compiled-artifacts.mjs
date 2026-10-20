export const previewLimit = 2048;
export const artifactFile = (slug, artifact, extension) => {
  if (!/^[a-z0-9-]+$/.test(slug) || !['rust_mir','chialisp','rue_source'].includes(artifact.backend)
    || typeof artifact.optimize_lir !== 'boolean' || !['clvm.hex','clvm.txt'].includes(extension)) throw new Error('Invalid compiled artifact identity');
  return `${slug}--${artifact.backend}--${artifact.optimize_lir ? 'on' : 'off'}.${extension}`;
};
export const preview = value => value.slice(0, previewLimit);
