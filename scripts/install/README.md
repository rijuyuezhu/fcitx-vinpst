# User installation

These scripts mutate the current user's XDG configuration/data directories and
session activation metadata. Use temporary-HOME smoke tests under
`../tests/install/` before changing a real profile.

- `install-user-ime.sh` installs, inspects, or removes the complete per-user
  daemon/Fcitx profile selected through `VINPUT_USER_PROFILE` and related
  environment variables.
- `install-user-activation-service.sh` is the narrower activation-service
  helper used by installation flows and tests.

The broad `just install-user`, `just user-status`, and `just user-remove`
recipes delegate to these scripts. Real-profile mutation must always be an
explicit user action.
