import { constants, copyFileSync, mkdirSync, statSync } from "node:fs";
import { dirname, resolve } from "node:path";

// Match correctionWorkerOptions and the relative paths in handy-dictionary.toml.
export function correctionRuntimeFiles(root, platform = process.platform) {
  const paths = [
    `rechprav/target/release/examples/correction_worker${platform === "win32" ? ".exe" : ""}`,
    "rechprav/handy-terms.toml",
    "rechprav/handy-dictionary.toml",
    "rechprav/artifacts/opencorpora-wordfreq/dictionary.rpt",
    "rechprav/DICTIONARY-NOTICE.md",
  ];
  return paths.map(relative => {
    const source = resolve(root, relative);
    try {
      if (!statSync(source).isFile()) throw new Error("not a file");
    } catch (error) {
      throw new Error(`Correction runtime file missing: ${source}. Build the worker with npm run build:correction and prepare/copy dictionary.rpt; see docs/rechprav/handy-integration.md.`, { cause: error });
    }
    return { source, relative, preserveExisting: relative.endsWith(".toml") };
  });
}

export function copyCorrectionRuntime(files, destination) {
  return files.map(({ source, relative, preserveExisting }) => {
    const target = resolve(destination, relative);
    mkdirSync(dirname(target), { recursive: true });
    try {
      copyFileSync(source, target, preserveExisting ? constants.COPYFILE_EXCL : 0);
    } catch (error) {
      if (!preserveExisting || error.code !== "EEXIST") throw error;
    }
    return target;
  });
}
