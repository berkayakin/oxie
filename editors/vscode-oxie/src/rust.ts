import * as path from 'node:path';
import * as vscode from 'vscode';
import type { OxieServer } from './server';

const SCHEME = 'oxie-rust';

/** `/dir/main.oxi` → `oxie-rust:/dir/main.oxi.rs`, a read-only document with the generated Rust. */
export function generatedRustUri(source: vscode.Uri): vscode.Uri {
    return source.with({ scheme: SCHEME, path: `${source.path}.rs` });
}

/** Shows the Rust generated for .oxi files beside them, updated while the source is edited. */
export class GeneratedRust implements vscode.TextDocumentContentProvider, vscode.Disposable {
    private readonly changed = new vscode.EventEmitter<vscode.Uri>();
    readonly onDidChange = this.changed.event;
    private readonly timers = new Map<string, ReturnType<typeof setTimeout>>();
    private readonly subscriptions: vscode.Disposable[];

    constructor(private readonly server: OxieServer) {
        this.subscriptions = [
            vscode.workspace.registerTextDocumentContentProvider(SCHEME, this),
            vscode.workspace.onDidChangeTextDocument((e) => this.sourceChanged(e.document)),
        ];
    }

    async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
        const source = uri.with({ scheme: 'file', path: uri.path.replace(/\.rs$/, '') });
        const rust = await this.server.generatedRust(source);
        if (rust === undefined) {
            return '// The Oxie language server is not running.\n';
        }
        return rust || `// No Rust was generated for ${path.basename(source.fsPath)}; see the Problems panel.\n`;
    }

    async show(uri?: vscode.Uri): Promise<void> {
        const source = uri ?? vscode.window.activeTextEditor?.document.uri;
        if (!source || source.scheme !== 'file' || !source.path.endsWith('.oxi')) {
            void vscode.window.showInformationMessage('Open an Oxie file first.');
            return;
        }
        const document = await vscode.workspace.openTextDocument(generatedRustUri(source));
        await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: true, preserveFocus: true });
    }

    /** Reloads every open view, for example after the server (re)started. */
    refreshAll(): void {
        for (const document of vscode.workspace.textDocuments) {
            if (document.uri.scheme === SCHEME) {
                this.changed.fire(document.uri);
            }
        }
    }

    private sourceChanged(document: vscode.TextDocument): void {
        if (document.languageId !== 'oxie') {
            return;
        }
        const view = generatedRustUri(document.uri);
        const key = view.toString();
        if (!vscode.workspace.textDocuments.some((d) => d.uri.toString() === key)) {
            return;
        }
        clearTimeout(this.timers.get(key));
        this.timers.set(key, setTimeout(() => {
            this.timers.delete(key);
            this.changed.fire(view);
        }, 300));
    }

    dispose(): void {
        this.timers.forEach(clearTimeout);
        this.subscriptions.forEach((s) => s.dispose());
        this.changed.dispose();
    }
}
