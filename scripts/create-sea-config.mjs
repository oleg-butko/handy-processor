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
const executable = process.argv[2];
const assets = Object.fromEntries(
  collectFiles(webDirectory).sort().map((fileName) => [
    `web/${fileName}`,
    `web-dist/${fileName}`,
  ]),
);
const generatedConfig = {
  ...sourceConfig,
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
