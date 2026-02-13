import * as path from 'path';
import * as os from 'os';
import * as fs from 'fs';
import { workspace, ExtensionContext } from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Executable,
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  try {
    const serverOptions: ServerOptions = getServerOptions(context);
    const clientOptions: LanguageClientOptions = {
      documentSelector: [{ scheme: 'file', language: 'avsc' }],
      synchronize: {
        fileEvents: workspace.createFileSystemWatcher('**/*.avsc')
      }
    };

    client = new LanguageClient(
      'avro-lsp',
      'Avro Language Server',
      serverOptions,
      clientOptions
    );

    client.start().catch((error) => {
      console.error('Failed to start Avro LSP server:', error);
    });

    console.log('Avro LSP extension activated');
  } catch (error) {
    console.error('Failed to activate Avro LSP extension:', error);
    throw error;
  }
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

function getServerOptions(context: ExtensionContext): ServerOptions {
  const config = workspace.getConfiguration('avro-lsp');
  const customPath = config.get<string>('server.path');

  let command: string;
  if (customPath) {
    command = customPath;
  } else {
    command = getBundledBinaryPath(context);
  }

  // Ensure binary has execute permissions on Unix-like systems
  if (process.platform !== 'win32') {
    try {
      fs.chmodSync(command, 0o755);
    } catch (err) {
      // Ignore errors - binary might already have correct permissions
    }
  }

  const executable: Executable = {
    command,
    args: [],
    options: {
      env: process.env
    }
  };

  return executable;
}

function getBundledBinaryPath(context: ExtensionContext): string {
  const platform = os.platform();
  const arch = os.arch();
  let binaryName: string;

  if (platform === 'win32') {
    binaryName = 'avro-lsp-win32-x64.exe';
  } else if (platform === 'linux') {
    binaryName = 'avro-lsp-linux-x64';
  } else if (platform === 'darwin') {
    // macOS - detect architecture
    if (arch === 'arm64') {
      binaryName = 'avro-lsp-darwin-arm64';
    } else if (arch === 'x64') {
      binaryName = 'avro-lsp-darwin-x64';
    } else {
      throw new Error(
        `Unsupported macOS architecture: ${arch}. Supported architectures are x64 (Intel) and arm64 (Apple Silicon). ` +
        `Please build from source and configure "avro-lsp.server.path" in settings.`
      );
    }
  } else {
    throw new Error(`Unsupported platform: ${platform}`);
  }

  const binaryPath = path.join(context.extensionPath, 'bin', binaryName);
  console.log(`Avro LSP binary path: ${binaryPath}`);
  
  // Check if binary exists
  if (!fs.existsSync(binaryPath)) {
    throw new Error(`Avro LSP binary not found at: ${binaryPath}`);
  }
  
  return binaryPath;
}
