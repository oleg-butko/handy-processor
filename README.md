# Handy local post-processor

A small OpenAI-compatible HTTP server for Handy's experimental
post-processing feature. It currently:

1. optionally converts the transcription to lowercase; and
2. optionally converts supported Russian cardinal numerals, including common case forms
   and scales through trillion, to digits, with conservative one-edit typo
   correction inside confirmed numeric phrases; and
3. optionally removes a period only when it is the final character (ignoring trailing
   whitespace);
4. stores each transformation in a size-limited local SQLite history that can
   be exported as text; and
5. provides a context-aware voice editor with named SQLite notes and
   persistent undo/redo history; and
6. shows the paginated history and configurable rule list in a React + Mantine
   web UI.

For example, `МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.` becomes
`мне нужно 22 файла`.

The server uses only Node.js built-ins at runtime and listens on
`127.0.0.1:11434` by default. It prints the browser URL when it starts.
See the [documentation index](docs/README.md) for build guides, architecture
notes and completed plans.

## Run it

Install Node.js 24.8 or newer, then run:

```bash
npm ci
npm run dev
```

Open the URL printed by the server, normally
`http://localhost:11434/`.

The **Голосовой редактор** tab uses CodeMirror for plain-text editing. Click
inside it before starting Handy post-processing so the server can use the
current document and cursor position. See the
[voice editor guide](docs/voice-editor.md) for contextual casing, undo/redo and
focus limitations.

For a compiled production-style run:

```bash
npm run build
npm start
```

With Node.js 26, build a SEA executable with embedded UI assets:

```bash
npm run build:sea:node26
```

See [the Windows Node.js 26 SEA guide](docs/instructions/windows-sea-node26.md)
for native and cross-build instructions.

The same commands work in Windows PowerShell. Keep that terminal open while
using Handy. If PowerShell's execution policy blocks `npm.ps1`, run the
equivalent `npm.cmd install` and `npm.cmd run dev` commands.

The port is read from `config.json` in the runtime directory. The server
always binds to `127.0.0.1`, because the UI has no authentication. The SQLite
database is stored as `handy-processor.sqlite3` next to that config. In a SEA
build both files are next to the executable.

## Configure Handy

In Handy:

1. Open **Settings > Advanced > Experimental Features** and enable
   **Post Processing**.
2. Open the new **Post-Processing** settings page.
3. Choose **Custom** as the provider.
4. Set **Base URL** to `http://127.0.0.1:11434/v1`.
5. Leave **API Key** empty. This server does not use authentication.
6. Refresh the model list and choose `handy-basic-transformer`. You can also
   enter that model name manually.
7. Create/select a prompt whose entire content is exactly `${output}`.
8. Use Handy's dedicated post-processing shortcut. The regular transcription
   shortcut intentionally bypasses post-processing.

The exact `${output}` prompt is important. Handy substitutes the transcription
into the prompt before making the request. If the prompt also contains
instructions, this simple server will transform those instructions along with
the transcription.

## Test without Handy

With the server running:

```bash
curl http://127.0.0.1:11434/health

curl http://127.0.0.1:11434/v1/models

curl http://127.0.0.1:11434/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"handy-basic-transformer","messages":[{"role":"user","content":"HELLO FROM HANDY."}],"stream":false}'
```

The final response contains:

```json
{
  "choices": [
    {
      "message": {
        "role": "assistant",
        "content": "hello from handy"
      }
    }
  ]
}
```

Run the automated tests with:

```bash
npm test
```

See [the Web UI and storage architecture](docs/web-ui-and-storage.md) for the
config format, SQLite schema and paginated API.

The Russian numeral dictionary is generated at development time and checked
into Git. Regenerate or verify it with Python 3:

```bash
npm run generate:numerals
npm run check:numerals
```

To validate the curated word forms with `pymorphy3`, create a development-only
virtual environment and run:

```bash
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-generator.txt
npm run validate:numerals-morphology
```

On Windows, use `py -m venv .venv`, activate `.venv\Scripts\Activate.ps1`,
install the same requirements, and invoke the generator directly with
`py scripts/generate_russian_numerals.py --check --validate-morphology`.

Python is not used by the running server or required by the SEA executable.
See [the research and implementation plan](docs/russian-number-normalization.md)
for the current scope and remaining normalization stages.
Development environment fixes are collected in
[the troubleshooting notes](docs/instructions/troubleshooting.md).

On Windows PowerShell, a quick live check is:

```powershell
$body = @{
  model = "handy-basic-transformer"
  messages = @(@{ role = "user"; content = "HELLO FROM HANDY." })
  stream = $false
} | ConvertTo-Json -Depth 4

Invoke-RestMethod `
  -Uri "http://127.0.0.1:11434/v1/chat/completions" `
  -Method Post `
  -ContentType "application/json" `
  -Body $body
```

## API implemented for Handy

- `GET /health` — basic process health check.
- `GET /v1/models` — lets Handy discover the local model.
- `POST /v1/chat/completions` — accepts Handy's OpenAI-compatible request and
  returns the transformed text in `choices[0].message.content`.
- `GET /` — serves the local React/Mantine history UI.
- `GET /api/transformations` — returns the transformation history with
  `page`/`pageSize` pagination.
- `GET /api/transformations/export` — downloads source/result pairs as UTF-8
  text.
- `GET/PATCH /api/settings/history` — reads or changes the SQLite history
  retention limit (50,000 by default).
- `GET /api/rules` — returns the transformation rule catalog and current state.
- `PATCH /api/rules/:ruleId` — enables or disables a rule for subsequent
  transformations.
- `GET/PUT /api/editor/draft` — loads or saves the editor draft.
- `GET/POST /api/editor/notes` and `GET/PUT/PATCH/DELETE
  /api/editor/notes/:noteId` — manage named notes and their undo/redo state.
- `PUT/DELETE /api/editor/sessions/:sessionId` — updates or removes ephemeral
  cursor context used by voice-editor rules.

Unknown OpenAI-compatible fields such as `reasoning_effort` are intentionally
ignored. This matches the request currently sent by Handy's Custom provider.

## Why this API shape

Inspection of the local Handy source shows that its Custom provider is an
OpenAI-compatible client rather than a custom webhook:

- Handy appends `/models` to the configured base URL to populate the model
  selector.
- It appends `/chat/completions` for each post-processing request.
- It sends the selected prompt as the final `user` message and reads the result
  from `choices[0].message.content`.
- For the Custom provider, it may also send `reasoning_effort: "none"`.

The relevant implementation is in `src-tauri/src/llm_client.rs` and
`src-tauri/src/actions.rs` in the Handy checkout. Because the Custom provider
does not request structured JSON output, no LLM or prompt interpretation is
needed for this deterministic processor.
