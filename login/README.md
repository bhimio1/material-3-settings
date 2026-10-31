# Login screen and boot animation

Installed to `/usr/share/m3-settings/login` and run from Settings → Login & boot
(or `m3-settings login …`). They came from the end4-pC fork and now follow
any shell Material 3 Settings integrates with, through
`$XDG_STATE_HOME/m3/style.json`.

| Path | What |
|---|---|
| `sddm/theme/` | the `end4-material` SDDM theme (Qt 6) |
| `sddm/install.sh` | installs and selects it, `--uninstall` restores the previous theme |
| `sddm/sync.sh` | copies wallpaper, colors, fonts and avatar to `/var/lib/end4-material-sddm`; the `m3-login-sync` systemd user path unit runs it when style.json changes |
| `sddm/test/preview.py` | renders the theme with `sddm-greeter --test-mode` (development only) |
| `plymouth/` | the `end4-pc` Plymouth theme: `render.py` draws the frames in your colors, `install.sh` installs it and rebuilds the initramfs |

The theme ids are the fork's, so logins set up by the fork keep working.

Everything in this folder is GPL-3.0-or-later (see `LICENSE-GPL-3.0`); the
emblem is illogical-impulse's logo from end-4/dots-hyprland. The Material
Symbols font in `sddm/theme/fonts/` is Apache-2.0, from Google.
