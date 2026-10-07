# Verification baseline

Run37548859824 at e19e667caa692eabbee84758910cb2f819ac5cba compiled the official-AST
exporter and passed all28 Rust tests, eight original Hurl requests, seven selected
cURL exports, one subset export and four corruption controls. The actual saved
POSIX text preserved argv bytes. Native result SHA256:
682c66a7b475b4934c7947493e1dea1a73153ca6a81f56236b54b9cb6fd97c11

The current workflow repeats the gate and additionally retains every subset and
negative-control capture, checking each intended one-field fault exactly before
positive-oracle rejection. Its outcome is established by that commit's CI run.
The exact43-dependency Cargo lock is retained at project root. No upstream source,
executable, registry cache or user request input is uploaded.
