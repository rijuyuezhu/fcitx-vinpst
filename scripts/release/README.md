# Release scripts

This directory owns the Arch package and signed release-candidate boundary.

Lightweight checks, also exposed through `just package-check`:

```sh
scripts/release/check-arch-install-script.sh
scripts/release/check-arch-pkgbuild.sh
scripts/release/check-rpm-spec.sh
scripts/release/check-release-manifest.sh
scripts/release/check-release-signature.sh
scripts/release/check-arch-release-candidate.sh
```

The complete release gate is:

```sh
scripts/release/run-arch-package-smoke.sh
scripts/release/run-rpm-package-smoke.sh
```

It builds the checked runtime bundle, package and synthetic upgrade archive,
then runs isolated pacman transaction/repository/signing tests and promotes a
verified release candidate. Test keys and synthetic `pkgrel=2` artifacts are
never production release inputs.

The package lifecycle contract installs three cooperating files:

- `package-session-common.sh`: ownership-verified session-bus discovery and minimal user environment construction;
- `package-upgrade-handoff.sh`: existing-owner-only dispatch into `vinput daemon handoff`;
- `package-remove-handoff.sh`: all-session removal preflight, guarded mutation, and activation rollback.

They are product lifecycle code, not developer convenience scripts.
