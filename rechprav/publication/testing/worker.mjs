import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { PublicationStore } from "../store.mjs";

const [database, operation, argument, point, marker, gate] = process.argv.slice(2);
const sleep = ms => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
const store = new PublicationStore(database, { testFault(current) {
  if (current === point) {
    writeFileSync(marker, current);
    while (true) sleep(1000); // Parent kills this real process at the transaction boundary.
  }
} });
try {
  if (gate) {
    writeFileSync(marker, "ready");
    const deadline = Date.now() + 15_000;
    while (!existsSync(gate)) {
      if (Date.now() > deadline) throw new Error("gate timeout");
      sleep(10);
    }
  }
  const result = operation === "request" ? store.request(JSON.parse(readFileSync(argument, "utf8")))
    : store.publish(argument);
  process.stdout.write(JSON.stringify(result));
} catch (error) { process.stderr.write(error.message); process.exitCode = 1; }
finally { store.close(); }
