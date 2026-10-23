import * as assert from 'node:assert';
import * as vscode from 'vscode';

const root = vscode.workspace.workspaceFolders![0].uri;

async function open(file: string): Promise<vscode.TextDocument> {
    const document = await vscode.workspace.openTextDocument(vscode.Uri.joinPath(root, file));
    await vscode.window.showTextDocument(document);
    return document;
}

/** The position of the `nth` occurrence of `text`, plus `offset` characters. */
function positionOf(document: vscode.TextDocument, text: string, offset = 0, nth = 1): vscode.Position {
    let at = -1;
    for (let i = 0; i < nth; i++) {
        at = document.getText().indexOf(text, at + 1);
        assert.ok(at >= 0, `${text} #${nth} not found`);
    }
    return document.positionAt(at + offset);
}

/** Retries `get` until it returns something other than false, undefined or an empty array. */
async function eventually<T>(what: string, get: () => T | undefined | Thenable<T | undefined>, timeout = 20_000): Promise<T> {
    const end = Date.now() + timeout;
    for (;;) {
        const value = await get();
        if (value && (!Array.isArray(value) || value.length)) {
            return value;
        }
        if (Date.now() > end) {
            throw new Error(`timed out waiting for ${what}`);
        }
        await new Promise((resolve) => setTimeout(resolve, 200));
    }
}

const label = (item: vscode.CompletionItem) => (typeof item.label === 'string' ? item.label : item.label.label);

suite('Oxie extension', () => {
    suiteSetup(async () => {
        await vscode.extensions.getExtension('oxie-lang.oxie')!.activate();
    });

    test('.oxi files are Oxie', async () => {
        const document = await open('scripts/hello.oxi');
        assert.strictEqual(document.languageId, 'oxie');
    });

    test('syntax errors are reported by the server', async () => {
        const document = await open('scripts/broken.oxi');
        const diagnostics = await eventually('diagnostics', () => {
            const found = vscode.languages.getDiagnostics(document.uri);
            return found.length ? found : undefined;
        });
        assert.ok(diagnostics.some((d) => d.source === 'oxie' && d.severity === vscode.DiagnosticSeverity.Error), JSON.stringify(diagnostics));
    });

    test('hover shows the generated Rust signature', async () => {
        const document = await open('scripts/hello.oxi');
        const hovers = await eventually('hover', () =>
            vscode.commands.executeCommand<vscode.Hover[]>('vscode.executeHoverProvider', document.uri, positionOf(document, 'greet("Oxie")', 1)),
        );
        const text = hovers.flatMap((h) => h.contents.map((c) => (typeof c === 'string' ? c : c.value))).join('\n');
        assert.match(text, /fn greet\(name: &str\) -> String/);
    });

    test('go to definition', async () => {
        const document = await open('scripts/hello.oxi');
        const locations = await vscode.commands.executeCommand<vscode.Location[]>('vscode.executeDefinitionProvider', document.uri, positionOf(document, 'greet("Oxie")', 1));
        assert.strictEqual(locations.length, 1);
        // the same URI as the open document, even though the test workspace's path has a symlink on macOS
        assert.strictEqual(locations[0].uri.toString(), document.uri.toString());
        assert.strictEqual(locations[0].range.start.line, positionOf(document, 'fun greet').line);
    });

    test('document symbols', async () => {
        const document = await open('scripts/hello.oxi');
        const symbols = await vscode.commands.executeCommand<vscode.DocumentSymbol[]>('vscode.executeDocumentSymbolProvider', document.uri);
        const names = symbols.map((s) => s.name);
        for (const name of ['Point', 'greet', 'main']) {
            assert.ok(names.includes(name), names.join(', '));
        }
    });

    test('completion has declarations, and each snippet once', async () => {
        const document = await open('scripts/hello.oxi');
        const list = await vscode.commands.executeCommand<vscode.CompletionList>('vscode.executeCompletionItemProvider', document.uri, positionOf(document, '    val p', 4));
        assert.ok(list.items.some((i) => label(i) === 'greet' && i.kind === vscode.CompletionItemKind.Function));
        const funSnippets = list.items.filter((i) => label(i) === 'fun' && i.kind === vscode.CompletionItemKind.Snippet);
        assert.strictEqual(funSnippets.length, 1);
    });

    test('formatting', async () => {
        const document = await open('scripts/messy.oxi');
        const edits = await vscode.commands.executeCommand<vscode.TextEdit[]>('vscode.executeFormatDocumentProvider', document.uri, { tabSize: 4, insertSpaces: true });
        const edit = new vscode.WorkspaceEdit();
        edit.set(document.uri, edits);
        await vscode.workspace.applyEdit(edit);
        assert.strictEqual(document.getText(), 'fun main() {\n    val x = 1\n    println(x)\n}\n');
        await vscode.commands.executeCommand('workbench.action.files.revert');
    });

    test('run and test code lenses', async () => {
        const document = await open('scripts/hello.oxi');
        const lenses = await vscode.commands.executeCommand<vscode.CodeLens[]>('vscode.executeCodeLensProvider', document.uri);
        const commands = lenses.map((l) => [l.range.start.line, l.command?.command, ...(l.command?.arguments ?? []).slice(1)]);
        const main = positionOf(document, 'fun main').line;
        const test = positionOf(document, 'test "').line;
        assert.deepStrictEqual(commands, [
            [main, 'oxie.run'],
            [main, 'oxie.runRelease'],
            [test, 'oxie.runTest', 'greets everyone'],
        ]);
    });

    test('generated Rust view follows edits', async () => {
        const document = await open('scripts/hello.oxi');
        await vscode.commands.executeCommand('oxie.showGeneratedRust', document.uri);
        const view = await eventually('generated Rust', () => vscode.workspace.textDocuments.find((d) => d.uri.scheme === 'oxie-rust'));
        assert.strictEqual(view.uri.path, `${document.uri.path}.rs`);
        assert.match(view.getText(), /pub\(crate\) fn greet\(name: &str\) -> String/);
        const edit = new vscode.WorkspaceEdit();
        edit.insert(document.uri, new vscode.Position(document.lineCount, 0), '\nfun added(): i32 = 42\n');
        await vscode.workspace.applyEdit(edit);
        await eventually('updated view', () => /fn added\(\) -> i32/.test(view.getText()));
        await vscode.window.showTextDocument(document);
        await vscode.commands.executeCommand('workbench.action.files.revert');
    });

    test('tasks for the project', async () => {
        const tasks = await vscode.tasks.fetchTasks({ type: 'oxie' });
        assert.deepStrictEqual(tasks.map((t) => t.name).sort(), ['build', 'check', 'fmt', 'run', 'test']);
        const build = tasks.find((t) => t.name === 'build')!;
        assert.strictEqual(build.group?.id, vscode.TaskGroup.Build.id);
        assert.deepStrictEqual((build.execution as vscode.ProcessExecution).args, ['build']);
    });

    test('rust { } blocks use the Rust grammar', async () => {
        const tokens = await vscode.commands.executeCommand<{ c: string; t: string }[]>('_workbench.captureSyntaxTokens', vscode.Uri.joinPath(root, 'scripts/hello.oxi'));
        const let_ = tokens.find((t) => t.c === 'let');
        assert.ok(let_, 'no `let` token');
        assert.match(let_.t, /meta\.embedded\.block\.rust .*storage\.type\.rust/);
        const main = tokens.find((t) => t.c === 'main');
        assert.match(main!.t, /^source\.oxie .*entity\.name\.function\.oxie/);
    });

    test('```oxie blocks in Markdown are highlighted', async () => {
        const tokens = await vscode.commands.executeCommand<{ c: string; t: string }[]>('_workbench.captureSyntaxTokens', vscode.Uri.joinPath(root, 'notes.md'));
        const fun = tokens.find((t) => t.c === 'fun');
        assert.ok(fun, 'no `fun` token');
        assert.match(fun.t, /meta\.embedded\.block\.oxie .*keyword\.other\.fn\.oxie/);
    });

    // slow: builds the script with cargo; run with OXIE_TEST_CARGO_CHECK=1
    test('rustc errors arrive after cargo check', async function () {
        if (!process.env.OXIE_TEST_CARGO_CHECK) {
            this.skip();
        }
        this.timeout(320_000);
        const document = await open('scripts/moved.oxi');
        const config = vscode.workspace.getConfiguration('oxie');
        await config.update('cargoCheck', true, vscode.ConfigurationTarget.Workspace);
        try {
            const diagnostic = await eventually('rustc diagnostics', () => vscode.languages.getDiagnostics(document.uri).find((d) => d.source === 'rustc'), 300_000);
            assert.match(diagnostic.message, /`nums` was moved/);
            assert.strictEqual(diagnostic.range.start.line, 5);
        } finally {
            await config.update('cargoCheck', false, vscode.ConfigurationTarget.Workspace);
        }
    });

    test('the server restarts', async () => {
        await vscode.commands.executeCommand('oxie.restartServer');
        const document = await open('scripts/hello.oxi');
        const symbols = await eventually('symbols after restart', () =>
            vscode.commands.executeCommand<vscode.DocumentSymbol[]>('vscode.executeDocumentSymbolProvider', document.uri),
        );
        assert.ok(symbols.some((s) => s.name === 'main'));
    });
});
