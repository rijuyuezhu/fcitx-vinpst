# Known limitations

This page distinguishes missing product capability from evidence that has not yet been collected in a real deployment.

## Release status

Vinpst has not published `0.1.0` yet. The final supported package/architecture matrix, production signing trust root, and public download instructions are still being completed.

## Desktop and application behavior

Normal commits use ordinary Fcitx integration and are broadly portable. Command replacement additionally depends on selected surrounding text or a usable primary selection.

Live evidence exists for GTK, Qt, Chromium/Ozone, GNOME Text Editor, kitty, and VS Code/Electron. This does not guarantee identical selection behavior in every application, sandbox, terminal, browser, or remote-desktop environment.

## Audio devices

PipeWire capture and device selection are implemented. Additional breadth is still needed for hot-plug switching across more physical microphones, Bluetooth profiles, USB devices, and unusual channel layouts.

Output ducking depends on the desktop audio-control path and is best-effort. Failure to duck must not prevent recording, and restoration is attempted after normal and error paths.

## Providers and networks

Deterministic tests cover local proxy routing, authenticated proxies, `NO_PROXY`, additional CA bundles, TLS verification, redirects, rate-limit/service errors, body limits, timeouts, DNS failure, and connection refusal.

The following are not yet claimed as production evidence:

- PAC;
- NTLM or Kerberos proxy authentication;
- enterprise TLS interception policy and certificate deployment;
- production credential rotation and custody;
- provider-specific hosted-service outages and rate-limit policies;
- a broad set of real hosted ASR and LLM vendors.

## Remote text interface

The HTTP/WebSocket runtime and a same-host browser path are implemented. A successful collector run from a separately confirmed physical network device is still an evidence gap.

## Languages

The retained Fcitx frontend provides English fallback and zh_CN localization. Additional interface languages are not currently part of the `0.1.0` parity target.

Recognition languages depend on the selected model/provider, not only on the interface locale.

## Long-duration operation

Repeated and bounded soak tests exist, including multi-cycle real-desktop runs. Hour-scale or longer unattended soak evidence is still incomplete.

## GUI accessibility

Keyboard interaction and common management flows are implemented, but final accessibility semantics and broader assistive-technology evidence remain part of the pre-release review.

## Compatibility policy

Vinpst uses its own package, executable, addon, D-Bus, service, and XDG identities. It does not migrate or replace another voice-input package. Interfaces created during pre-release development may change before `0.1.0` when doing so improves the product.
