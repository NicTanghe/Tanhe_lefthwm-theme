# Tanhe LeftWM theme

A glass-styled LeftWM desktop built around Eww, Picom, Rofi, and a shared
Dracula GTK palette.

The `up` script links the GTK 3/4 palette from `gtk/` into the user's
configuration. Replaced GTK files receive a safety backup under
`~/.local/state/tanhe-leftwm-theme/gtk-backup`; `down` does not remove the
glass GTK configuration.
