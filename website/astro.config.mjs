// @ts-check
import fs from 'node:fs';
import path from 'node:path';
import starlight from '@astrojs/starlight';
import { defineConfig } from 'astro/config';
import starlightLinksValidator from 'starlight-links-validator';
import starlightThemeNext from 'starlight-theme-next';
import { repo, sidebar, sources, syncDocs } from './scripts/sync-docs.mjs';

const github = 'https://github.com/oxie-lang/oxie';

// the VS Code extension's grammar, so examples are highlighted the same way as in the editor
const grammar = JSON.parse(fs.readFileSync(path.join(repo, 'editors/vscode-oxie/syntaxes/oxie.tmLanguage.json'), 'utf8'));

// Picks the same tab (Oxie, Rust or Output) in every example on the page, or Oxie where an
// example has no such tab, and remembers it, keeping the clicked example where it was on screen.
const tabScript = `
function oxieShowTab(value) {
	for (const tabs of document.querySelectorAll('.oxie-tabs')) {
		const input = tabs.querySelector('input[value="' + value + '"]') || tabs.querySelector('input[value="oxie"]');
		if (input) input.checked = true;
	}
}
document.addEventListener('change', (e) => {
	const input = e.target;
	if (!(input instanceof HTMLInputElement) || !input.closest('.oxie-tabs')) return;
	const top = input.getBoundingClientRect().top;
	oxieShowTab(input.value);
	window.scrollBy(0, input.getBoundingClientRect().top - top);
	try { localStorage.setItem('oxie-tab', input.value); } catch {}
});
document.addEventListener('DOMContentLoaded', () => {
	let tab = null;
	try { tab = localStorage.getItem('oxie-tab'); } catch {}
	if (tab) oxieShowTab(tab);
});`;

/** Generates the pages from docs/ before the site is built, and again when a source changes. */
function oxieDocs() {
	return {
		name: 'oxie-docs',
		hooks: {
			'astro:config:setup': ({ config }) => {
				// Rendered pages are cached with links to the code styles, whose names change with the
				// code settings above but don't invalidate the cache, so start from a clean one.
				for (const dir of [new URL('.astro/', config.root), config.cacheDir]) fs.rmSync(new URL('data-store.json', dir), { force: true });
				syncDocs();
			},
			/** @param {{ server: import('vite').ViteDevServer }} options */
			'astro:server:setup': ({ server }) => {
				server.watcher.add(sources());
				server.watcher.on('change', (file) => sources().includes(file) && syncDocs());
			},
		},
	};
}

export default defineConfig({
	site: 'https://oxie.bkymz.net',
	integrations: [
		oxieDocs(),
		starlight({
			title: 'Oxie',
			description: 'A programming language for the Rust ecosystem: a concise, familiar syntax that compiles to plain Rust.',
			logo: { light: '../docs/assets/oxie-logo-light.svg', dark: '../docs/assets/oxie-logo-dark.svg', replacesTitle: true },
			favicon: '/favicon.svg',
			social: [{ icon: 'github', label: 'GitHub', href: github }],
			editLink: { baseUrl: `${github}/edit/main/website/` },
			customCss: ['@fontsource-variable/instrument-sans', '@fontsource/commit-mono', './src/styles/custom.css'],
			expressiveCode: {
				// functions, variables, parameters and types each get their own color
				themes: ['one-dark-pro', 'one-light'],
				// the default (5.5) washes the theme's colors out; 4.5 is WCAG AA
				minSyntaxHighlightingColorContrast: 4.5,
				shiki: { langs: [{ ...grammar, name: 'oxie', aliases: ['oxi'] }] },
			},
			head: [{ tag: 'script', content: tabScript }],
			sidebar,
			plugins: [starlightThemeNext(), starlightLinksValidator()],
		}),
	],
	vite: { server: { fs: { allow: [repo] } } },
});
