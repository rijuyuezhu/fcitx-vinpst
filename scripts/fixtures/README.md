# Fixtures

Fixtures implement independent process/protocol boundaries used by deterministic
and live tests:

- command-ASR JSON/WAV helpers;
- the legacy raw-PCM-to-WAV compatibility bridge;
- loopback OpenAI-compatible ASR and text-provider servers, with optional fixed loopback ports for same-endpoint restart/CA-file replacement and controlled response delays/padding for timeout and bounded-body gates, and optional 3xx `Location` headers for fail-closed redirect gates;
- deterministic WAV generation.

They are executable test instruments, not product daemons. Callers must provide
explicit temporary output paths and must not persist credentials or recognized
user text in tracked files.

`remote-text-input-client.py` authenticates one deterministic input client and finalizes exact text for remote-text process smokes. `http-asr-proxy-fixture.py` accepts one absolute-form proxied ASR request, optionally requires proxy-URL Basic authentication, records only Bearer/Basic schemes and payload metadata, and returns a deterministic transcription without retaining credentials. `https-connect-proxy-fixture.py` requires one Basic-authenticated CONNECT request, optionally protects its own listener with TLS, tunnels encrypted bytes to a fixed upstream, and records only target/auth metadata, proxy TLS state, and directional byte counts; it never records tunneled payloads or proxy credentials. `https-intercept-proxy-fixture.py` terminates one client TLS exchange with a CA-signed certificate, verifies and re-establishes TLS to a fixed upstream, relays exactly one bounded HTTP message in each direction, and persists only TLS versions plus header/body byte counts; decrypted headers, bodies, Bearer values, proxy credentials, prompts, text, and audio are never written by the interception fixture.
