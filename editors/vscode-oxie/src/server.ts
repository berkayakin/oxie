import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import * as vscode from 'vscode';
import {
    ExecuteCommandRequest,
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    State,
    WorkDoneProgress,
    WorkDoneProgressBegin,
    WorkDoneProgressEnd,
    WorkDoneProgressReport,
} from 'vscode-languageclient/node';

const EXE = process.platform === 'win32' ? 'oxie.exe' : 'oxie';

function cargoBin(): string {
    return path.join(process.env.CARGO_HOME || path.join(os.homedir(), '.cargo'), 'bin');
}

function isFile(p: string): boolean {
    try {
        return fs.statSync(p).isFile();
    } catch {
        return false;
    }
}

function onPath(name: string): string | undefined {
    const dirs = (process.env.PATH ?? '').split(path.delimiter).filter((d) => d);
    return [...dirs, cargoBin()].map((d) => path.join(d, name)).find(isFile);
}

/** The `oxie` executable: the `oxie.path` setting, else `oxie` on the PATH, else Cargo's bin directory. */
export function findOxie(): string | undefined {
    let configured = vscode.workspace.getConfiguration('oxie').get<string>('path', '').trim();
    if (!configured) {
        return onPath(EXE);
    }
    if (!/[\\/]/.test(configured)) {
        return onPath(configured);
    }
    const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    configured = configured.replace(/^~(?=$|[\\/])/, os.homedir());
    if (folder) {
        configured = configured.replace(/\$\{workspaceFolder\}/g, folder);
    }
    const resolved = path.isAbsolute(configured) || !folder ? configured : path.join(folder, configured);
    return isFile(resolved) ? resolved : undefined;
}

/** The environment for oxie processes. Cargo's bin directory is added to the PATH when it is missing
 * (VS Code started outside a login shell), so the server and builds find `cargo`. */
export function oxieEnv(): Record<string, string> {
    const env: Record<string, string> = {};
    for (const [key, value] of Object.entries(process.env)) {
        if (value !== undefined) {
            env[key] = value;
        }
    }
    const key = Object.keys(env).find((k) => k.toUpperCase() === 'PATH') ?? 'PATH';
    const dirs = (env[key] ?? '').split(path.delimiter).filter((d) => d);
    if (!dirs.includes(cargoBin())) {
        env[key] = [...dirs, cargoBin()].join(path.delimiter);
    }
    return env;
}

/** Runs `oxie lsp` and connects it to VS Code. */
export class OxieServer implements vscode.Disposable {
    private client: LanguageClient | undefined;
    private starting: Promise<void> = Promise.resolve();
    private readonly output = vscode.window.createOutputChannel('Oxie Language Server', { log: true });
    private readonly status = vscode.languages.createLanguageStatusItem('oxie.server', { language: 'oxie' });
    private endCheck: (() => void) | undefined;

    constructor() {
        this.status.name = 'Oxie Language Server';
        this.status.text = 'oxie';
    }

    start(): Promise<void> {
        this.starting = this.starting.then(() => this.doStart()).catch((e) => this.output.appendLine(`Starting the server failed: ${e}`));
        return this.starting;
    }

    async restart(): Promise<void> {
        await this.stop();
        await this.start();
    }

    async stop(): Promise<void> {
        await this.starting;
        const client = this.client;
        this.client = undefined;
        this.checkEnded();
        if (client) {
            await client.dispose().catch(() => undefined);
        }
    }

    showOutput(): void {
        this.output.show();
    }

    /** The Rust generated for an .oxi file, or undefined if the server isn't running. */
    async generatedRust(source: vscode.Uri): Promise<string | undefined> {
        await this.starting;
        const client = this.client;
        if (!client?.isRunning()) {
            return undefined;
        }
        const uri = client.code2ProtocolConverter.asUri(source);
        return client.sendRequest(ExecuteCommandRequest.type, { command: 'oxie.showRust', arguments: [uri] });
    }

    private async doStart(): Promise<void> {
        if (this.client) {
            return;
        }
        const command = findOxie();
        if (!command) {
            this.missing();
            return;
        }
        const serverOptions: ServerOptions = { command, args: ['lsp'], options: { env: oxieEnv() } };
        const clientOptions: LanguageClientOptions = {
            documentSelector: [{ scheme: 'file', language: 'oxie' }],
            initializationOptions: { cargoCheck: vscode.workspace.getConfiguration('oxie').get('cargoCheck', true) },
            outputChannel: this.output,
            middleware: {
                // Snippets come from snippets/oxie.json (they work without the server), so the
                // server's copies would only show up twice.
                provideCompletionItem: async (document, position, context, token, next) => {
                    const result = await next(document, position, context, token);
                    const keep = (item: vscode.CompletionItem) => item.kind !== vscode.CompletionItemKind.Snippet;
                    if (Array.isArray(result)) {
                        return result.filter(keep);
                    }
                    if (result) {
                        result.items = result.items.filter(keep);
                    }
                    return result;
                },
            },
        };
        const client = new LanguageClient('oxie', 'Oxie Language Server', serverOptions, clientOptions);
        // `cargo check` progress; the server reports it without creating the token first
        client.onProgress(WorkDoneProgress.type, 'oxie-check', (value) => this.checkProgress(value));
        client.onDidChangeState((e) => {
            if (e.newState === State.Stopped) {
                this.checkEnded();
            }
        });
        this.client = client;
        try {
            await client.start();
        } catch (e) {
            this.client = undefined;
            this.status.severity = vscode.LanguageStatusSeverity.Error;
            this.status.text = 'oxie failed to start';
            this.status.detail = String(e);
            this.status.command = { title: 'Show Output', command: 'oxie.showOutput' };
            return;
        }
        const version = client.initializeResult?.serverInfo?.version;
        this.status.severity = vscode.LanguageStatusSeverity.Information;
        this.status.text = version ? `oxie ${version}` : 'oxie';
        this.status.detail = command;
        this.status.command = { title: 'Restart', command: 'oxie.restartServer' };
    }

    private missing(): void {
        const configured = vscode.workspace.getConfiguration('oxie').get<string>('path', '').trim();
        this.status.severity = vscode.LanguageStatusSeverity.Error;
        this.status.text = 'oxie not found';
        this.status.detail = configured ? `oxie.path: ${configured}` : 'install it with cargo install oxie';
        this.status.command = { title: 'Retry', command: 'oxie.restartServer' };
        const message = configured
            ? `The oxie command set in oxie.path was not found: ${configured}`
            : 'The Oxie language server needs the oxie command. Install it with "cargo install oxie", or set oxie.path.';
        const install = 'Install with Cargo';
        const settings = 'Open Settings';
        void vscode.window.showErrorMessage(message, ...(configured ? [] : [install]), settings).then(async (choice) => {
            if (choice === install) {
                await this.install();
            } else if (choice === settings) {
                await vscode.commands.executeCommand('workbench.action.openSettings', 'oxie.path');
            }
        });
    }

    /** Runs `cargo install oxie` in a terminal and starts the server when it succeeds. */
    private async install(): Promise<void> {
        const execution = new vscode.ProcessExecution('cargo', ['install', 'oxie'], { env: oxieEnv() });
        const task = new vscode.Task({ type: 'oxie', command: 'install' }, vscode.TaskScope.Workspace, 'cargo install oxie', 'oxie', execution);
        const started = await vscode.tasks.executeTask(task);
        const listener = vscode.tasks.onDidEndTaskProcess((e) => {
            if (e.execution === started) {
                listener.dispose();
                if (e.exitCode === 0) {
                    void this.restart();
                }
            }
        });
    }

    private checkProgress(value: WorkDoneProgressBegin | WorkDoneProgressReport | WorkDoneProgressEnd): void {
        if (value.kind === 'begin') {
            this.checkEnded();
            this.status.busy = true;
            const done = new Promise<void>((resolve) => (this.endCheck = resolve));
            void vscode.window.withProgress({ location: vscode.ProgressLocation.Window, title: value.title }, () => done);
        } else if (value.kind === 'end') {
            this.checkEnded();
        }
    }

    private checkEnded(): void {
        this.endCheck?.();
        this.endCheck = undefined;
        this.status.busy = false;
    }

    dispose(): void {
        void this.stop();
        this.status.dispose();
        this.output.dispose();
    }
}
