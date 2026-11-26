# dmgbuild settings for Rec's DMG: Rec on the left, an arrow, Applications on
# the right, in a window that fits them. scripts/release-macos.sh runs:
#
#   dmgbuild -s scripts/dmg/settings.py -D app=<Rec.app> -D background=<tiff> Rec <out.dmg>
#
# The positions match the arrow in scripts/dmg/background.swift.

import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
app_name = os.path.basename(app)

format = "UDZO"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = os.path.join(app, "Contents", "Resources", "icon.icns")

background = defines["background"]  # noqa: F821
window_rect = ((200, 120), (640, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
icon_size = 128
text_size = 13
icon_locations = {
    app_name: (160, 180),
    "Applications": (480, 180),
}
