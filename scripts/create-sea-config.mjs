import {
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { join, relative, resolve, sep } from "node:path";

const root = resolve(import.meta.dirname, "..");
const webDirectory = resolve(root, "web-dist");
const outputDirectory = resolve(root, "sea-build");
const sourceConfigPath = resolve(root, "sea-config.node26.json");
const outputConfigPath = resolve(outputDirectory, "sea-config.node26.generated.json");

function parseArguments(arguments_) {
  let executable;
  let output;

  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--output") {
      output = arguments_[index + 1];
      if (!output || output.startsWith("--")) {
        throw new Error("--output requires a path");
      }
      index += 1;
    } else if (argument.startsWith("--")) {
      throw new Error(`Unknown option: ${argument}`);
    } else if (executable === undefined) {
      executable = argument;
    } else {
      throw new Error(`Unexpected argument: ${argument}`);
    }
  }

  return { executable, output };
}

function collectFiles(directory, current = directory) {
  const files = [];
  for (const entry of readdirSync(current, { withFileTypes: true })) {
    const path = join(current, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(directory, path));
    } else if (entry.isFile()) {
      files.push(relative(directory, path).split(sep).join("/"));
    }
  }
  return files;
}

const sourceConfig = JSON.parse(readFileSync(sourceConfigPath, "utf8"));
const { executable, output } = parseArguments(process.argv.slice(2));
const assets = Object.fromEntries(
  collectFiles(webDirectory).sort().map((fileName) => [
    `web/${fileName}`,
    `web-dist/${fileName}`,
  ]),
);
const generatedConfig = {
  ...sourceConfig,
  ...(output ? { output } : {}),
  ...(executable ? { executable } : {}),
  assets,
};

mkdirSync(outputDirectory, { recursive: true });
writeFileSync(
  outputConfigPath,
  `${JSON.stringify(generatedConfig, null, 2)}\n`,
  "utf8",
);

console.log(
  `Generated ${relative(root, outputConfigPath)} with ${Object.keys(assets).length} assets`,
);
