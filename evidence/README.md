# Verification status

The official-AST exporter compiled in run37546782829.26 of27 original Rust tests
passed; the remaining test incorrectly used an unescaped Hurl comment marker as
a URL fragment. The corrected fixture and new comment case need the next run.
Wire parity has not run. The actual resolved Cargo lock is retained at project root.
Python gate scripts pass syntax compilation locally. Generated CI reports belong
here; no upstream source, executable, registry cache or user request input is uploaded.
