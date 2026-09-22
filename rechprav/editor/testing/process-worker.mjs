import { readFileSync } from "node:fs";
import { CaptureStore } from "../capture-store.mjs";

const [database, requestPath, fault] = process.argv.slice(2);
const request = JSON.parse(readFileSync(requestPath, "utf8"));
const store = new CaptureStore(database, {
  testFault(point) { if (point === fault) process.kill(process.pid, "SIGKILL"); },
});
function run() {
  let result;
  try { result = { ok: true, receipt: store.append(request) }; }
  catch (error) { result = { ok: false, error: error.message }; }
  finally { store.close(); }
  if (process.send) process.send(result, () => process.disconnect());
  else process.stdout.write(JSON.stringify(result));
}
if (process.send) { process.send({ ready: true }); process.once("message", run); }
else run();
