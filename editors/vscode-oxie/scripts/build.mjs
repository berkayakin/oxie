// Bundles the extension into out/extension.js. `--production` minifies it and writes the LICENSE
// file for the package; `--watch` rebuilds on changes.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import * as esbuild from 'esbuild';

process.chdir(fileURLToPath(new URL('..', import.meta.url)));
const production = process.argv.includes('--production');

const options = {
  entryPoints: ['src/extension.ts'],
  outfile: 'out/extension.js',
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['vscode'],
  minify: production,
  sourcemap: !production,
  logLevel: 'info',
};

if (process.argv.includes('--watch')) {
  await (await esbuild.context(options)).watch();
} else {
  await esbuild.build(options);
}

if (production) {
  const mit = readFileSync('../../LICENSE-MIT', 'utf8');
  const apache = readFileSync('../../LICENSE-APACHE', 'utf8');
  writeFileSync('LICENSE', `Oxie for VS Code is licensed under either of the MIT license or the Apache License,\nVersion 2.0, at your option.\n\n${mit}\n\n${apache}`);
}
