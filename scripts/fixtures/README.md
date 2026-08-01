# Fixtures

Fixtures implement independent process/protocol boundaries used by deterministic
and live tests:

- command-ASR JSON/WAV helpers;
- the legacy raw-PCM-to-WAV compatibility bridge;
- loopback OpenAI-compatible ASR and text-provider servers;
- deterministic WAV generation.

They are executable test instruments, not product daemons. Callers must provide
explicit temporary output paths and must not persist credentials or recognized
user text in tracked files.

`remote-text-input-client.py` authenticates one deterministic input client and finalizes exact text for remote-text process smokes. `http-asr-proxy-fixture.py` accepts one absolute-form proxied ASR request, records only the Bearer scheme and payload digest, and returns a deterministic transcription without retaining credentials.
