# Handy local post-processor

A small OpenAI-compatible HTTP server for Handy's experimental
post-processing feature. It currently:

1. converts the transcription to lowercase; and
2. converts simple Russian cardinal numerals through 999 to digits; and
3. removes a period only when it is the final character (ignoring trailing
   whitespace).

For example, `МНЕ НУЖНО ДВАДЦАТЬ ДВА ФАЙЛА.` becomes
`мне нужно 22 файла`.

The server uses only Node.js built-ins at runtime and listens on
`127.0.0.1:11434` by default.

## Run it

Install Node.js 20 or newer, then run:

```bash
npm install
npm run dev
```

For a compiled production-style run:

```bash
npm run build
npm start
```

The same commands work in Windows PowerShell. Keep that terminal open while
using Handy. If PowerShell's execution policy blocks `npm.ps1`, run the
equivalent `npm.cmd install` and `npm.cmd run dev` commands.

Optional environment variables:

- `HOST` changes the bind address (default: `127.0.0.1`).
- `PORT` changes the port (default: `11434`).

Keeping the default host makes the server accessible only from the same
computer.

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

The Russian numeral dictionary is generated at development time and checked
into Git. Regenerate or verify it with Python 3:

```bash
npm run generate:numerals
npm run check:numerals
```

Python is not used by the running server or required by the SEA executable.
See [the research and implementation plan](docs/russian-number-normalization.md)
for the current scope and planned morphology support.

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
