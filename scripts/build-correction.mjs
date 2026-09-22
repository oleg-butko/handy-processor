import { spawnSync } from 'node:child_process';
import { homedir } from 'node:os';
import { join } from 'node:path';

const args = ['build', '--manifest-path', 'rechprav/Cargo.toml', '--release', '--locked', '--offline', '--example', 'correction_worker'];
let result = spawnSync(process.env.CARGO ?? 'cargo', args, { stdio: 'inherit' });
if (result.error?.code === 'ENOENT' && !process.env.CARGO) {
  result = spawnSync(join(homedir(), '.local', 'bin', process.platform === 'win32' ? 'cargo.exe' : 'cargo'), args, { stdio: 'inherit' });
}
if (result.error) console.error('Не удалось запустить Cargo. Укажите путь через CARGO.', result.error.message);
process.exitCode = result.status ?? 1;
