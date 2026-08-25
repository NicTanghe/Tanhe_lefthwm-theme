# Tanhe LeftWM theme

A glass-styled LeftWM desktop built around Eww, Picom, Rofi, and a shared
Dracula GTK palette. The current configuration targets an X11 CachyOS/Arch
installation while retaining fallbacks for common desktop helpers.

The `up` script permanently links the GTK 3/4 palette from `gtk/` into the
user's configuration, starts the Eww bar and Picom, then loads the LeftWM
theme. Replaced GTK files receive a safety backup under
`~/.local/state/tanhe-leftwm-theme/gtk-backup`; `down` does not remove the
Glass GTK configuration.

The theme expects LeftWM, Eww (the patched binary is included), Picom, Rofi,
Alacritty, NetworkManager, PipeWire/WirePlumber, and the MesloLGS Nerd Font.
Optional integrations such as Dunst, Blueman, and several file managers are
detected at runtime.
