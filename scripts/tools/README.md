# Developer tools

This directory contains manual diagnostics and benchmarks that are useful while
investigating behavior but are not release or CI gates.

`bench-capture-cold-start.sh` summarizes structured capture-start timing from
journal or saved log input. Its output is diagnostic evidence; it does not
change capture policy or replace the deterministic analyzer smoke under
`../tests/asr/`.
