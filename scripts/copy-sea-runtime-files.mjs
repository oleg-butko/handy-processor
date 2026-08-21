import { copyFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const outputDirectory = resolve(root, "sea-build");

mkdirSync(outputDirectory, { recursive: true });
copyFileSync(resolve(root, "config.json"), resolve(outputDirectory, "config.json"));
