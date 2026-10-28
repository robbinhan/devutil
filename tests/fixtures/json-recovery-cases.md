# JSON Recovery Test Corpus

这是一份脱敏的合成样本集，用来定义 JSON 工具的行为边界。

- `accept`：输入本身合法，只做格式化或压缩。
- `recover`：可以安全自动还原，必须记录执行过的转换。
- `inspect`：存在嵌套 JSON、URL 或 Base64 候选，默认保留原文并提示用户。
- `reject`：无法确定用户意图，不应静默猜测。

每个 `expected` 都表示规范化后的紧凑 JSON；展示层可以另行美化。

## A. 标准 JSON：accept

### A-001 — object root

```text
{"user":{"id":42,"name":"Alice","active":true},"tags":["macOS","dev"]}
```

```json
{"user":{"id":42,"name":"Alice","active":true},"tags":["macOS","dev"]}
```

### A-002 — array root

```text
[{"id":1,"name":"A"},{"id":2,"name":"B"}]
```

```json
[{"id":1,"name":"A"},{"id":2,"name":"B"}]
```

### A-003 — escaped Unicode

```text
{"text":"\u4f60\u597d \ud83d\ude80","line":"first\nsecond"}
```

```json
{"text":"你好 🚀","line":"first\nsecond"}
```

## B. 转义与嵌套：recover / inspect

### B-001 — JSON encoded as a string

```text
"{\"user\":{\"id\":42,\"name\":\"Alice\"}}"
```

```json
{"user":{"id":42,"name":"Alice"}}
```

Expected transform: `unwrap-json-string`, then `unescape-json-string`.

### B-002 — double-escaped JSON

```text
"\"{\\\"ok\\\":true,\\\"items\\\":[1,2,3]}\""
```

```json
{"ok":true,"items":[1,2,3]}
```

Expected transform: repeat safe string decoding until the result is a JSON value or no progress is possible.

### B-003 — JSON nested in a field

```text
{"event":"login","payload":"{\"userId\":42,\"success\":true}"}
```

```json
{"event":"login","payload":"{\"userId\":42,\"success\":true}"}
```

Expected behavior: accept the outer JSON; show `payload` as a nested-JSON candidate. Do not mutate the parent silently.

### B-004 — JSON inside a Markdown fence

````text
```json
{"ok":true,"data":[1,2]}
```
````

```json
{"ok":true,"data":[1,2]}
```

Expected transform: `strip-markdown-fence`.

## C. 容错结构：recover

### C-001 — single quotes and unquoted keys

```text
{name: 'Alice', roles: ['admin', 'viewer']}
```

```json
{"name":"Alice","roles":["admin","viewer"]}
```

### C-002 — trailing commas

```text
{"name":"Alice","items":[1,2,],}
```

```json
{"name":"Alice","items":[1,2]}
```

### C-003 — comments

```text
{
  // user id
  "id": 42,
  /* account state */
  "active": true
}
```

```json
{"id":42,"active":true}
```

### C-004 — JSONP wrapper

```text
callback({"ok":true,"data":[1,2]});
```

```json
{"ok":true,"data":[1,2]}
```

### C-005 — truncated containers

```text
{"users":[{"id":1,"name":"A"},{"id":2,"name":"B"
```

```json
{"users":[{"id":1,"name":"A"},{"id":2,"name":"B"}]}
```

Expected transform: close only containers whose opening tokens are unambiguous; report the repair.

### C-006 — newline-delimited JSON

```text
{"id":1,"name":"A"}
{"id":2,"name":"B"}
```

```json
[{"id":1,"name":"A"},{"id":2,"name":"B"}]
```

Expected transform: `detect-ndjson`, then wrap as an array. Keep a separate NDJSON output mode if possible.

## D. 编码候选：inspect

### D-001 — URL encoded JSON

```text
%7B%22user%22%3A%7B%22id%22%3A42%7D%7D
```

```json
{"user":{"id":42}}
```

Expected behavior: detect URL-decoded candidate and show the transform before applying it.

### D-002 — Base64 encoded JSON

```text
eyJvayI6dHJ1ZSwiY291bnQiOjJ9
```

```json
{"ok":true,"count":2}
```

Expected behavior: detect Base64 candidate only when the decoded result is valid UTF-8 and parses as JSON.

### D-003 — log prefix with JSON payload

```text
2026-08-09T12:00:01Z INFO response={"ok":true,"count":2}
```

```json
{"ok":true,"count":2}
```

Expected behavior: extract the balanced JSON span, preserve the complete log line, and show the extracted range.

## E. 不应静默猜测：reject

### E-001 — conflicting values

```text
{"id":42,"id":43}
```

Expected behavior: parse only if duplicate-key policy is explicitly selected; otherwise show a warning and preserve both occurrences in the source view.

### E-002 — ambiguous missing separator

```text
{"a":"first" "b":"second"}
```

Expected behavior: report the missing separator. Do not auto-repair unless the user enables aggressive repair.

### E-003 — ambiguous numeric literal

```text
{"value":01}
```

Expected behavior: report an invalid numeric literal; do not silently convert it to `1`.

### E-004 — unterminated string

```text
{"message":"this text was cut off
```

Expected behavior: report an unterminated string. A closing quote cannot be inferred safely.

## Acceptance criteria

1. Valid JSON is never changed semantically.
2. Every automatic transformation is visible and reversible.
3. `recover` cases produce the documented `expected` value.
4. `inspect` cases never mutate the source without user action.
5. `reject` cases show a useful location and reason instead of returning a guessed object.
6. All cases run both from the editor and from pasted clipboard text.
