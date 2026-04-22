# rice-cooker-helper

Privileged helper for [rice-cooker](../backend). Invoked via `pkexec` by
the unprivileged main binary.

## Scope

Three subcommands, each with a tight input-validation contract:

- `install-repo-packages <pkg1> <pkg2> ...`
  → `pacman -S --needed --noconfirm <pkgs>`
- `install-built-packages <path1> <path2> ...`
  → `pacman -U --needed --noconfirm <paths>`
- `remove-packages <pkg1> <pkg2> ...`
  → `pacman -Rns --noconfirm <pkgs>`

Nothing else. This binary does not parse the catalog, does not touch
`$HOME`, does not network. Its whole job is to validate arguments and
hand them to pacman.

## Security

- Every package name must match `^[a-zA-Z0-9@._+-]+$`, reject leading
  `-` (would look like a pacman option), reject `> 256` bytes.
- Every built-package path must be absolute, end in `.pkg.tar.zst` or
  `.pkg.tar.xz`, contain no `..` segments, be a regular file (not a
  symlink), be owned by the invoking user (`PKEXEC_UID`), and
  canonicalize to a path under `/home/<user>/.cache/rice-cooker/aur/` or
  `/tmp/`.
- On any validation failure → exit non-zero before touching pacman.

## Install

```sh
cargo build --release -p rice-cooker-helper
sudo install -m 0755 target/release/rice-cooker-helper /usr/bin/
sudo install -m 0644 polkit/so.butterfly.ricecooker.policy \
    /usr/share/polkit-1/actions/
```

The polkit policy uses `auth_admin_keep` so repeated invocations within
~5 minutes of each other share one password prompt — giving install a
single-prompt experience even though it makes multiple `pkexec` calls.

## Testing

Unit tests fuzz the validation rules:

```sh
cargo test -p rice-cooker-helper
```

End-to-end testing requires a polkit agent and root access and is best
done on a real system or VM.
