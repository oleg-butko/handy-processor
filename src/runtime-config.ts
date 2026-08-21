import {
  existsSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { isSea } from "node:sea";

export const CONFIG_FILE_NAME = "config.json";
export const DATABASE_FILE_NAME = "handy-processor.sqlite3";

export interface RuntimeConfig {
  readonly port: number;
}

const DEFAULT_CONFIG: RuntimeConfig = { port: 11434 };

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseConfig(contents: string, configPath: string): RuntimeConfig {
  let value: unknown;
  try {
    value = JSON.parse(contents);
  } catch (error) {
    throw new Error(`Invalid JSON in ${configPath}`, { cause: error });
  }

  if (!isObject(value)) {
    throw new Error(`${configPath} must contain a JSON object`);
  }

  const port = value.port;
  if (
    typeof port !== "number" ||
    !Number.isInteger(port) ||
    port < 1 ||
    port > 65535
  ) {
    throw new Error(`${configPath}: port must be an integer from 1 to 65535`);
  }

  return { port };
}

export function resolveRuntimeDirectory(): string {
  const override = process.env.HANDY_PROCESSOR_RUNTIME_DIR?.trim();
  if (override) {
    return resolve(override);
  }

  return isSea() ? dirname(process.execPath) : process.cwd();
}

export function loadRuntimeConfig(runtimeDirectory: string): RuntimeConfig {
  const configPath = join(runtimeDirectory, CONFIG_FILE_NAME);
  if (!existsSync(configPath)) {
    writeFileSync(configPath, `${JSON.stringify(DEFAULT_CONFIG, null, 2)}\n`, {
      encoding: "utf8",
      flag: "wx",
    });
  }

  return parseConfig(readFileSync(configPath, "utf8"), configPath);
}

export function getDatabasePath(runtimeDirectory: string): string {
  return join(runtimeDirectory, DATABASE_FILE_NAME);
}
