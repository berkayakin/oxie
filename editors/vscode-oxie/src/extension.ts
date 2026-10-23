import * as vscode from 'vscode';
import { GeneratedRust } from './rust';
import { OxieTaskProvider, RunLenses, TASK_TYPE, runFile } from './run';
import { OxieServer } from './server';

let server: OxieServer | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
    const oxie = new OxieServer();
    server = oxie;
    const rust = new GeneratedRust(oxie);
    const lenses = new RunLenses();
    const restart = async () => {
        await oxie.restart();
        rust.refreshAll();
    };
    context.subscriptions.push(
        oxie,
        rust,
        lenses,
        vscode.languages.registerCodeLensProvider({ language: 'oxie', scheme: 'file' }, lenses),
        vscode.tasks.registerTaskProvider(TASK_TYPE, new OxieTaskProvider()),
        vscode.commands.registerCommand('oxie.run', (uri?: vscode.Uri) => runFile(uri, 'run')),
        vscode.commands.registerCommand('oxie.runRelease', (uri?: vscode.Uri) => runFile(uri, 'run', ['--release'])),
        vscode.commands.registerCommand('oxie.test', (uri?: vscode.Uri) => runFile(uri, 'test')),
        vscode.commands.registerCommand('oxie.runTest', (uri: vscode.Uri, name: string) => runFile(uri, 'test', [], name)),
        vscode.commands.registerCommand('oxie.showGeneratedRust', (uri?: vscode.Uri) => rust.show(uri)),
        vscode.commands.registerCommand('oxie.restartServer', restart),
        vscode.commands.registerCommand('oxie.showOutput', () => oxie.showOutput()),
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (vscode.workspace.isTrusted && (e.affectsConfiguration('oxie.path') || e.affectsConfiguration('oxie.cargoCheck'))) {
                void restart();
            }
        }),
        vscode.workspace.onDidGrantWorkspaceTrust(async () => {
            lenses.refresh();
            await oxie.start();
            rust.refreshAll();
        }),
    );
    // Without trust only the grammar and snippets work: the server builds the code, running its build scripts.
    if (vscode.workspace.isTrusted) {
        await oxie.start();
        rust.refreshAll();
    }
}

export function deactivate(): Promise<void> | undefined {
    return server?.stop();
}
