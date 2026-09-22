import { writeFileSync } from "node:fs";
import { PublicationStore, pack, readText } from "./store.mjs";

const [command, ...args] = process.argv.slice(2);
let store;
try {
  let output;
  if (command === "pack" && args.length === 3) {
    const bundle = pack(args[0], args[1]);
    writeFileSync(args[2], JSON.stringify(bundle) + "\n", { flag: "wx", mode: 0o600 });
    output = { bundle_file: args[2] };
  } else if (command === "create" && args.length === 2) {
    store = PublicationStore.create(args[0], args[1]); output = store.snapshot();
  } else if (["request", "publish", "snapshot"].includes(command)
    && args.length === (command === "snapshot" ? 1 : 2)) {
    store = new PublicationStore(args[0]);
    output = command === "request" ? store.request(JSON.parse(readText(args[1])))
      : command === "publish" ? store.publish(args[1]) : store.snapshot();
  } else throw new Error("usage: pack COMPOSITION_DIR MANUAL.toml NEW_BUNDLE.json | create DB SCOPE | request DB REQUEST.json | publish DB ID | snapshot DB");
  process.stdout.write(JSON.stringify(output, null, 2) + "\n");
} catch (error) { process.stderr.write(`${error.message}\n`); process.exitCode = 1; }
finally { store?.close(); }
