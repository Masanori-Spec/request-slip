# RequestSlip

A small native CLI that turns a supported literal Hurl request file into reviewable POSIX cURL command text and a JSON argv receipt, without making the requests.

This is a source-only Rust/Linux project. It requires a build toolchain and libxml2; it is **not a single-file browser app**. After it is built, converting files works offline. There is no execution command, HTTP client, URL import, server, account, automatic configuration discovery or persistent storage in the product.

**Native feasibility is not established yet.** The first public source snapshot exists to run the real parser and wire-capture gate described below. It has not yet compiled or passed native parity in this workspace. Do not treat a source review as a working-release claim.

[The first CI run](https://github.com/Masanori-Spec/request-slip/actions/runs/37545545297) stopped before compilation at the license guard: ryu1.0.23 declares `Apache-2.0 OR BSL-1.0`, and Boost Software License1.0 was absent from the reviewed identifiers. Its [exact upstream license](https://github.com/dtolnay/ryu/blob/f0b52bb194befe6fd242154f2182fafd43a819b8/LICENSE-BOOST) is now checked by hash. The corrected guard also saves resolved lock/provenance metadata before reporting an unreviewed license. A successful retry is still required; no native assertions were weakened.

## Why this small tool

[Hurl issue2666](https://github.com/Orange-OpenSource/hurl/issues/2666) asks for cURL export without first executing requests, including [paid or destructive endpoints](https://github.com/Orange-OpenSource/hurl/issues/2666#issuecomment-2050144854). Hurl already has `--curl FILE` for exporting requests **as they execute**. That native feature can reflect runtime values and remains the appropriate existing tool for that workflow. [curlconverter](https://github.com/curlconverter/curlconverter) translates cURL onward to other languages.

RequestSlip is a modest static subset exporter with entry selection and an argv review receipt. It is not a general Hurl converter, session replay tool, or a new conversion algorithm. Demand beyond the public issue and commercial usefulness are unvalidated.

The initially considered hurlfmt JSON bridge was rejected: its pinned8.0.1 formatter drops `[BasicAuth]`, so JSON alone cannot detect that omitted behavior. RequestSlip uses the **unmodified official `hurl_core=8.0.1` parser and public AST** instead. No Hurl grammar is reimplemented. The native gate compares actual plain/auth hurlfmt exports to reproduce that omission and requires RequestSlip to reject the original auth source.

## Build and use

The first supported build target is Ubuntu22.04 x86_64 with Rust1.98.1, a C linker, pkg-config, libxml2 headers and libclang. Other platforms are untested. Building requires registry access; conversion after building does not. This source snapshot contains no prebuilt binaries or installer.

```sh
# With the documented toolchain and system dependencies already installed:
cargo +1.98.1 build --release
./target/release/request-slip --help
./target/release/request-slip requests.hurl --select 1,3 --format json > review.argv.json
./target/release/request-slip requests.hurl --select 1,3 --format text > requests.curl.txt
```

The CLI reads only the path you give it (or `-` for stdin) and writes to stdout. Shell redirection above creates files; the CLI itself does not overwrite inputs or save state. Review the receipt and command text before deciding whether to run anything yourself. Exported cURL commands can perform destructive or billable operations when manually run.

Selections are 1-based, unique and emitted in original source order. The JSON receipt includes source SHA-256, selected entry numbers, source lines, declared headers, literal body, argv and POSIX command text. Unselected request payloads are absent from both exports. All source entries are checked for unsupported features before any output, including unselected entries. The source hash and original entry count still describe the whole imported file.

## Deliberately bounded contract

- Literal GET, POST, PUT, PATCH, DELETE and OPTIONS requests
- Absolute lowercase HTTP(S) URLs with ASCII DNS/IPv4 authorities, no credentials, fragments, dot path segments, IPv6, backslashes or URL glob characters
- Ordered duplicate and empty headers; repeated query/form values; UTF-8 oneline, plain multiline and raw bodies
- Parameter/header names restricted to ASCII letters, digits, dash, dot, underscore and tilde
- At most256KiB input,64 entries,32 selected entries,64 pairs per section,8KiB literal field,16KiB final URL,64KiB body and1MiB actual serialized output
- Raw input also has a conservative128 opening-bracket budget and1–6-digit Unicode-escape-looking bound, including inside comments/raw text, before calling the recursive parser

Auth, cookies, options, multipart, files, binary, JSON/XML/GraphQL bodies, placeholders, assertions/captures, unsupported methods and unknown AST forms block output. Response expectations must be bare status lines and are explicitly marked as omitted. No response is obtained, so response-derived cookies or other session effects cannot be reproduced. Each command is independent; the file is not promised to be an equivalent executed session.

**Every literal `<` is rejected before parsing**, including in comments, headers or delimited bodies. This intentionally conservative non-XML restriction prevents the parser from entering its libxml SAX branch. BOM and unsupported control characters are also rejected. This is not a claim to support all static Hurl files.

Literal values come from the official AST. Raw bodies use the AST's original source spelling, as the pinned Hurl runner does; ordinary oneline strings use decoded values. Plain multiline `\n` remains two characters. JSON bodies are excluded rather than lose large integers, duplicate keys or original body whitespace through JSON reserialization.

The exporter matches pinned Hurl8.0.1 query/form value escaping, empty-header syntax and implicit Content-Type handling. It emits `curl --disable` first, disables URL globbing, preserves accepted path text, suppresses implicit Expect, and uses the pinned Hurl user agent unless explicitly supplied. It does not promise identical TLS, protocol negotiation, proxy settings, environment behavior, network responses, or every implicit transport header. It adds no insecure TLS option or automatic redirect following. Only POSIX shell quoting is supplied; PowerShell/cmd.exe are unsupported.

## Native verification design

`scripts/native_gate.py` accepts **no input path or endpoint arguments**. Only the committed fixture with a fixed SHA-256 may reach request execution. Its `.invalid` host is replaced with a newly allocated `127.0.0.1` capture listener. HOME/CURL_HOME are temporary, proxy variables are not inherited, and generated cURL argv is checked against a strict option/destination allowlist before subprocess execution. No shell executes an HTTP command. A separate controlled synthetic shell check uses only `set`/`printf` to capture quoted argv.

The fixture has eight original Hurl requests and seven selected exports: existing and repeated query values, plus/space/Japanese encoding, duplicate/empty headers, literal apostrophes/`$()`/backticks, repeated form values containing`=&@`, raw body beginning`@`, plain multiline, decoded oneline, empty body and a trailing-question-mark URL. One final request is physically omitted from exports. A second selection exports only entry2.

The actual CLI's JSON/text stdout is saved to files and re-read. The official Hurl executable runs the original fixed fixture; system cURL runs only the checked product argv. Captured method, raw request target, selected duplicate/empty/default header pairs and exact UTF-8 body bytes must match independently authored literal expectations **and** each other. Four actual-request mutations change method, query ordering, one duplicate header and one body byte; each must fail the fixed oracle.

Official Hurl/hurlfmt8.0.1 asset SHA-256 is `cac7c4670d69444db120edb21fe06c97ba8c80dcc52279957c8dd18f05fb0c06`. The core release commit is `a39c7c43457ba2aa8edad833f33f9afe28444838`; all44 core Rust source files are checked against official Git blob IDs. CI records the resolved Cargo lock, dependency license/checksum records, compiler, system libxml2 and cURL versions. The first feasibility run resolves the lock; a verified release must retain and repeat with that exact lock.

The product imports only `hurl_core::parser`/AST/types, never core file-input helpers or Hurl runner/client APIs. Read-only source inspection found file I/O in the separate core input module and native XML parsing in the excluded branch. The CLI reads only its explicit file/stdin. Unsupported/invalid input produces an error without source excerpts or partial stdout.

CI uploads verification metadata and synthetic exported text/JSON only. It excludes registry source, compilers, target directories, official binaries and system libraries. Original project code has no license grant; dependency notices retain their separate scope.

## 日本語

Hurlファイルの対応範囲に含まれるリクエストを選び、実行せずにcURLコマンドのテキストと確認用JSONへ変換する小さなCLIです。ブラウザーアプリではなく、Rustとlibxml2を使ったビルドが必要です。ビルド後の変換はオフラインで行えます。

認証・Cookie・変数・外部ファイル・JSON/XML本文などは対応外として出力を止めます。Hurlの既存`--curl`は実際に送ったリクエストの出力機能で、本ツールはその置き換えではありません。出力を自分で実行すると通信や課金・更新処理が発生する可能性があるため、内容を確認してください。現在は初回のネイティブ検証前で、動作確認済みの完成版とはしていません。
