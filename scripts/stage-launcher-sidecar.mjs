import { copyFileSync, mkdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { basename, resolve } from "node:path";

const projectRoot = resolve(import.meta.dirname, "..");
const source = resolve(
  projectRoot,
  process.argv[3] ?? "sea-build/handy-processor.exe",
);

function detectTargetTriple() {
  try {
    return execFileSync("rustc", ["--print", "host-tuple"], {
      encoding: "utf8",
    }).trim();
  } catch {
    if (process.platform === "win32") {
      return process.arch === "arm64"
        ? "aarch64-pc-windows-msvc"
        : "x86_64-pc-windows-msvc";
    }

    throw new Error(
      "Cannot detect the Rust target. Install Rust or pass a target triple.",
    );
  }
}

const targetTriple = process.argv[2] ?? detectTargetTriple();
const extension = targetTriple.includes("windows") ? ".exe" : "";
const destinationDirectory = resolve(
  projectRoot,
  "launcher/src-tauri/binaries",
);
const destination = resolve(
  destinationDirectory,
  `handy-processor-server-${targetTriple}${extension}`,
);

mkdirSync(destinationDirectory, { recursive: true });
copyFileSync(source, destination);

console.log(`Staged ${basename(source)} as ${destination}`);
