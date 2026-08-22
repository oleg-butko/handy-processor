import {
  closeSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readSync,
  readFileSync,
} from "node:fs";
import { basename, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const projectRoot = resolve(import.meta.dirname, "..");

function requireFile(path, description) {
  if (!existsSync(path)) {
    throw new Error(`${description} not found: ${path}`);
  }
}

function requirePortableExecutable(path, description) {
  requireFile(path, description);
  const signatureBuffer = Buffer.alloc(2);
  const descriptor = openSync(path, "r");
  try {
    readSync(descriptor, signatureBuffer, 0, signatureBuffer.length, 0);
  } finally {
    closeSync(descriptor);
  }
  const signature = signatureBuffer.toString("ascii");
  if (signature !== "MZ") {
    throw new Error(`${description} is not a Windows PE executable: ${path}`);
  }
}

function findLauncherExecutable() {
  const releaseDirectory = resolve(
    projectRoot,
    "launcher/src-tauri/target/release",
  );
  const candidates = [
    resolve(releaseDirectory, "HandyProcessor.exe"),
    resolve(releaseDirectory, "handy-processor-launcher.exe"),
  ];
  const launcher = candidates.find(existsSync);
  if (launcher === undefined) {
    throw new Error(
      `Launcher executable not found. Checked: ${candidates.join(", ")}`,
    );
  }
  return launcher;
}

function validateConfig(path) {
  requireFile(path, "Runtime config");
  const config = JSON.parse(readFileSync(path, "utf8"));
  if (
    typeof config !== "object" ||
    config === null ||
    !Number.isInteger(config.port) ||
    config.port < 1 ||
    config.port > 65_535
  ) {
    throw new Error(`${path}: port must be an integer from 1 to 65535`);
  }
}

export function packageWindowsPortable({
  launcher,
  server,
  config,
  destination,
}) {
  requirePortableExecutable(launcher, "Launcher");
  requirePortableExecutable(server, "Server SEA");
  validateConfig(config);
  mkdirSync(destination, { recursive: true });

  const files = [
    [launcher, resolve(destination, "HandyProcessor.exe")],
    [server, resolve(destination, "handy-processor-server.exe")],
    [config, resolve(destination, "config.json")],
  ];
  for (const [source, target] of files) {
    copyFileSync(source, target);
  }

  return files.map(([, target]) => target);
}

function main() {
  const destination = resolve(
    projectRoot,
    "release/handy-processor-windows-x64",
  );
  const files = packageWindowsPortable({
    launcher: findLauncherExecutable(),
    server: resolve(projectRoot, "sea-build/handy-processor.exe"),
    config: resolve(projectRoot, "sea-build/config.json"),
    destination,
  });

  console.log(`Portable release is ready: ${destination}`);
  for (const file of files) {
    console.log(`- ${basename(file)}`);
  }
}

if (
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  main();
}
