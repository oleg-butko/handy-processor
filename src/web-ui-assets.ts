import { readFileSync, readdirSync } from "node:fs";
import { extname, join, relative, sep } from "node:path";
import { getAsset, getAssetKeys, isSea } from "node:sea";

export interface WebAsset {
  readonly body: Buffer;
  readonly contentType: string;
}

export interface WebUiAssets {
  get(requestPath: string): WebAsset | undefined;
}

const CONTENT_TYPES: Readonly<Record<string, string>> = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".wasm": "application/wasm",
  ".webp": "image/webp",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
};

function contentType(fileName: string): string {
  return CONTENT_TYPES[extname(fileName)] ?? "application/octet-stream";
}

function collectFiles(directory: string, current = directory): string[] {
  const files: string[] = [];

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

function loadAssetsFromDirectory(directory: string): Map<string, WebAsset> {
  const assets = new Map<string, WebAsset>();
  for (const fileName of collectFiles(directory)) {
    assets.set(`/${fileName}`, {
      body: readFileSync(join(directory, ...fileName.split("/"))),
      contentType: contentType(fileName),
    });
  }
  return assets;
}

function loadEmbeddedAssets(): Map<string, WebAsset> {
  const assets = new Map<string, WebAsset>();
  for (const key of getAssetKeys()) {
    if (!key.startsWith("web/")) {
      continue;
    }

    const fileName = key.slice("web/".length);
    assets.set(`/${fileName}`, {
      body: Buffer.from(getAsset(key)),
      contentType: contentType(fileName),
    });
  }
  return assets;
}

export function loadWebUiAssets(directory: string): WebUiAssets {
  const assets = isSea()
    ? loadEmbeddedAssets()
    : loadAssetsFromDirectory(directory);

  const index = assets.get("/index.html");
  if (index === undefined) {
    throw new Error("Web UI is missing index.html; run npm run build:ui");
  }

  return {
    get(requestPath) {
      if (requestPath === "/") {
        return index;
      }

      return (
        assets.get(requestPath) ??
        (extname(requestPath) === "" ? index : undefined)
      );
    },
  };
}
