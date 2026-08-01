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

`remote-text-input-client.py` authenticates one deterministic input client and finalizes exact text for remote-text process smokes. `http-asr-proxy-fixture.py` accepts one absolute-form proxied ASR request, optionally requires proxy-URL Basic authentication, records only Bearer/Basic schemes and payload metadata, and returns a deterministic transcription without retaining credentials. `http-chat-proxy-fixture.py` applies the same redaction boundary to one OpenAI-compatible chat-completions request and records only request shape, payload digest, and input-presence metadata. `https-connect-proxy-fixture.py` requires one Basic-authenticated CONNECT request, optionally protects its own listener with TLS, tunnels encrypted bytes to a fixed upstream, and records only target/auth metadata, proxy TLS state, and directional byte counts; it never records tunneled payloads or proxy credentials.
