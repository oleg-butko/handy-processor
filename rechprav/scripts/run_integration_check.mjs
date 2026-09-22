// Run from the repository root after npm run build. Uses no user database.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { CorrectionWorker } from '../../dist/correction-worker.js';
import { transformTranscription, transformTranscriptionAsync } from '../../dist/transform.js';
import { DEFAULT_ENABLED_RULE_IDS, TRANSFORMATION_RULE_IDS } from '../../dist/transformation-rules.js';

const output = resolve(process.argv[2]);
mkdirSync(output); // Do not overwrite a previous run.
const binary = resolve('rechprav/target/release/examples', process.platform === 'win32' ? 'correction_worker.exe' : 'correction_worker');
const config = resolve('rechprav/handy-terms.toml');
const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const worker = new CorrectionWorker({ executable: binary, config });
const summary = [];
const enabled = new Set([...DEFAULT_ENABLED_RULE_IDS, TRANSFORMATION_RULE_IDS.correction]);
const inputs = {
  'user-reported': 'rechprav/data/user-examples/2026-09-21/corpus.jsonl',
  'phrase-controls': 'rechprav/data/fixtures/user-phrases/corpus.jsonl',
  'family-controls': 'rechprav/data/fixtures/term-families/corpus.jsonl',
};
const predictions = [];
try {
  for (const [dataset, path] of Object.entries(inputs)) {
    const records = readFileSync(path, 'utf8').trim().split('\n').map(line => JSON.parse(line));
    let exactBefore = 0, exactAfter = 0, harmedCorrectRows = 0;
    for (const row of records) {
      const before = transformTranscription(row.source).text;
      const expected = transformTranscription(row.gold).text;
      const result = await transformTranscriptionAsync(row.source, enabled, undefined, worker);
      assert.equal(result.appliedRules[0].correction.complete, true);
      const trace = result.appliedRules[0].correction;
      for (const d of trace.decisions) for (const c of d.candidates) {
        assert.ok(Math.abs(Object.values(c.parts).reduce((a, b) => a + b, 0) - c.score) < 1e-8);
      }
      exactBefore += Number(before === expected);
      exactAfter += Number(result.text === expected);
      harmedCorrectRows += Number(row.source === row.gold && result.text !== expected);
      predictions.push({ dataset, id: row.id, source: row.source, gold: row.gold, expectedAfterFormatting: expected,
        before, after: result.text, appliedRules: result.appliedRules });
    }
    summary.push({ dataset, records: records.length, exactBefore, exactAfter, harmedCorrectRows });
  }
  writeFileSync(join(output, 'predictions.jsonl'), predictions.map(r => JSON.stringify(r)).join('\n') + '\n');
  writeFileSync(join(output, 'summary.json'), JSON.stringify({
    scope: 'Integration check on known development examples, not independent quality evidence; gold is formatted by existing rules; corpora are not pooled',
    profiles: summary, worker: worker.status(),
    commit: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(),
    hashes: Object.fromEntries([binary, config, 'rechprav/handy-dictionary.toml', 'rechprav/artifacts/opencorpora-wordfreq/dictionary.rpt',
      ...Object.values(inputs), 'rechprav/examples/correction_worker.rs', 'src/correction-worker.ts', 'src/transform.ts'].map(p => [p, sha(p)])),
  }, null, 2) + '\n');
  console.log(summary);
} finally { worker.close(); }
