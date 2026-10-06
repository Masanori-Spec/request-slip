# Verification status

The official-AST exporter compiled and all28 Rust tests passed in run37547806289.
Actual selected JSON/text exports were produced. The wire harness stopped before
requests on an unsupported Hurl CLI option; the invocation is now corrected to
the pinned CLI's --no-output. Wire parity is still pending a successful retry.
The actual resolved Cargo lock is retained at project root.
Python gate scripts pass syntax compilation locally. Generated CI reports belong
here; no upstream source, executable, registry cache or user request input is uploaded.
