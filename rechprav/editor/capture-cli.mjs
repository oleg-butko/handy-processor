import { readFileSync, statSync } from "node:fs";
import { CaptureStore } from "./capture-store.mjs";

const [command, database, argument, ...extra] = process.argv.slice(2);
let store;
try {
  if (!database || extra.length || !["create", "append", "export", "snapshot"].includes(command)
    || (["append", "export"].includes(command) !== Boolean(argument))) {
    throw new Error("usage: node capture-cli.mjs create|snapshot DB | append DB REQUEST.json | export DB NEW_DIRECTORY");
  }
  store = command === "create" ? CaptureStore.create(database) : new CaptureStore(database);
  let output;
  if (command === "append") {
    if (statSync(argument).size > 64 * 1024) throw new Error("request file exceeds 64 KiB");
    const bytes = readFileSync(argument);
    if (bytes.length > 64 * 1024) throw new Error("request file exceeds 64 KiB");
    output = store.append(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
  } else if (command === "export") output = store.exportSnapshot(argument);
  else output = store.snapshot();
  process.stdout.write(JSON.stringify(output, null, 2) + "\n");
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exitCode = 1;
} finally { store?.close(); }
