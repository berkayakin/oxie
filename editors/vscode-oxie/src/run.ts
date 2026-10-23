import * as fs from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { findOxie, oxieEnv } from './server';

export const TASK_TYPE = 'oxie';

interface OxieTaskDefinition extends vscode.TaskDefinition {
    command: string;
    args?: string[];
}

function isOxieProject(dir: string): boolean {
    try {
        return fs.readFileSync(path.join(dir, 'Cargo.toml'), 'utf8').includes('oxie-build');
    } catch {
        return false;
    }
}

/** The directory to run a file in: its project's root for a file under `src/`, else its own directory. */
function runDirectory(file: string): string {
    for (let dir = path.dirname(file); ; dir = path.dirname(dir)) {
        if (fs.existsSync(path.join(dir, 'Cargo.toml'))) {
            return file.startsWith(path.join(dir, 'src') + path.sep) ? dir : path.dirname(file);
        }
        if (path.dirname(dir) === dir) {
            return path.dirname(file);
        }
    }
}

function oxieTask(definition: OxieTaskDefinition, scope: vscode.WorkspaceFolder | vscode.TaskScope, name: string, cwd: string | undefined): vscode.Task {
    const args = [definition.command, ...(definition.args ?? [])];
    const execution = new vscode.ProcessExecution(findOxie() ?? 'oxie', args, { cwd, env: oxieEnv() });
    const task = new vscode.Task(definition, scope, name, TASK_TYPE, execution);
    task.presentationOptions = { clear: true, showReuseMessage: false };
    return task;
}

/** `oxie <command> [options] file.oxi [test name]` for a file, or for the active editor's file. */
export async function runFile(uri: vscode.Uri | undefined, command: string, options: string[] = [], testName?: string): Promise<void> {
    const document = uri ? await vscode.workspace.openTextDocument(uri) : vscode.window.activeTextEditor?.document;
    if (!document || document.languageId !== 'oxie' || document.uri.scheme !== 'file') {
        void vscode.window.showInformationMessage('Open an Oxie file first.');
        return;
    }
    if (document.isDirty) {
        await document.save();
    }
    const file = document.uri.fsPath;
    const args = [...options, file, ...(testName === undefined ? [] : [testName])];
    const cwd = runDirectory(file);
    const label = testName === undefined ? [command, ...options, path.basename(file)].join(' ') : `${command} "${testName}"`;
    const folder = vscode.workspace.getWorkspaceFolder(document.uri);
    if (!vscode.workspace.workspaceFolders?.length) {
        // tasks need a workspace: a single file opened on its own runs in a terminal
        const terminal = vscode.window.createTerminal({ name: `oxie ${label}`, cwd, env: oxieEnv(), shellPath: findOxie() ?? 'oxie', shellArgs: [command, ...args] });
        terminal.show();
        return;
    }
    await vscode.tasks.executeTask(oxieTask({ type: TASK_TYPE, command, args }, folder ?? vscode.TaskScope.Workspace, label, cwd));
}

/** `oxie build`, `run`, `test`, `check` and `fmt` for each Oxie project in the workspace, and tasks.json entries of type `oxie`. */
export class OxieTaskProvider implements vscode.TaskProvider {
    provideTasks(): vscode.Task[] {
        const tasks: vscode.Task[] = [];
        for (const folder of vscode.workspace.workspaceFolders ?? []) {
            if (!isOxieProject(folder.uri.fsPath)) {
                continue;
            }
            for (const command of ['build', 'run', 'test', 'check', 'fmt']) {
                const task = oxieTask({ type: TASK_TYPE, command }, folder, command, folder.uri.fsPath);
                if (command === 'build') {
                    task.group = vscode.TaskGroup.Build;
                } else if (command === 'test') {
                    task.group = vscode.TaskGroup.Test;
                }
                tasks.push(task);
            }
        }
        return tasks;
    }

    resolveTask(task: vscode.Task): vscode.Task | undefined {
        const definition = task.definition as OxieTaskDefinition;
        if (typeof definition.command !== 'string') {
            return undefined;
        }
        const scope = task.scope ?? vscode.TaskScope.Workspace;
        const cwd = typeof scope === 'object' ? scope.uri.fsPath : vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
        return oxieTask(definition, scope, task.name, cwd);
    }
}

// `fun main(` at the start of a line, after annotations and modifiers
const MAIN = /^(?:@[\w:.]+(?:\([^)]*\))?\s+)*(?:(?:pub|public|private|internal|async|suspend|unsafe)\s+)*(?:fun|fn)\s+main\s*\(/;
const TEST = /^\s*(?:async\s+)?test\s+"((?:[^"\\]|\\.)*)"/;

/** ▶ Run above `fun main` and ▶ Run Test above each `test "..."` block. */
export class RunLenses implements vscode.CodeLensProvider, vscode.Disposable {
    private readonly changed = new vscode.EventEmitter<void>();
    readonly onDidChangeCodeLenses = this.changed.event;

    provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
        if (!vscode.workspace.isTrusted) {
            return [];
        }
        const lenses: vscode.CodeLens[] = [];
        for (let i = 0; i < document.lineCount; i++) {
            const line = document.lineAt(i);
            if (MAIN.test(line.text)) {
                lenses.push(new vscode.CodeLens(line.range, { title: '$(play) Run', command: 'oxie.run', arguments: [document.uri] }));
                lenses.push(new vscode.CodeLens(line.range, { title: 'Run --release', command: 'oxie.runRelease', arguments: [document.uri] }));
            }
            const test = TEST.exec(line.text);
            if (test) {
                lenses.push(new vscode.CodeLens(line.range, { title: '$(play) Run Test', command: 'oxie.runTest', arguments: [document.uri, test[1]] }));
            }
        }
        return lenses;
    }

    refresh(): void {
        this.changed.fire();
    }

    dispose(): void {
        this.changed.dispose();
    }
}
