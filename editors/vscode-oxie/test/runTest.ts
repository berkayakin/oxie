// Runs the integration tests in VS Code on a copy of test/fixtures, an Oxie project with scripts.
// VSCODE_EXECUTABLE picks the VS Code to run (default: download the latest stable release);
// OXIE_PATH picks the `oxie` command (default: the one on the PATH).
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
    const extension = path.resolve(__dirname, '../..');
    const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'oxie-vscode-'));
    fs.cpSync(path.join(extension, 'test/fixtures'), workspace, { recursive: true });
    fs.writeFileSync(path.join(workspace, 'Cargo.toml'), '[package]\nname = "fixture"\nversion = "0.1.0"\nedition = "2021"\n\n[build-dependencies]\noxie-build = "0.1.0"\n');
    const settings: Record<string, unknown> = { 'oxie.cargoCheck': false };
    if (process.env.OXIE_TEST_TRACE) {
        settings['oxie.trace.server'] = 'verbose';
    }
    if (process.env.OXIE_PATH) {
        settings['oxie.path'] = path.resolve(process.env.OXIE_PATH);
    }
    fs.mkdirSync(path.join(workspace, '.vscode'));
    fs.writeFileSync(path.join(workspace, '.vscode/settings.json'), JSON.stringify(settings, null, 2));
    try {
        await runTests({
            vscodeExecutablePath: process.env.VSCODE_EXECUTABLE || undefined,
            extensionDevelopmentPath: extension,
            extensionTestsPath: path.resolve(__dirname, 'suite/index'),
            launchArgs: [workspace, '--disable-extensions'],
        });
    } finally {
        fs.rmSync(workspace, { recursive: true, force: true });
    }
}

main().catch((e) => {
    console.error(e);
    process.exit(1);
});
