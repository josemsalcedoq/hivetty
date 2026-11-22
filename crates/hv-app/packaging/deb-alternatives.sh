#!/bin/sh
# Adds maintainer scripts to the .deb that cargo-packager built, so hivetty registers as an
# `x-terminal-emulator` alternative (F6-08). cargo-packager 0.11 has no option for them.
# Usage: deb-alternatives.sh <package.deb>
set -eu

deb="$1"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

dpkg-deb -R "$deb" "$work/pkg"

cat > "$work/pkg/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "configure" ]; then
    update-alternatives --install /usr/bin/x-terminal-emulator x-terminal-emulator /usr/bin/hivetty 20
fi
EOF

cat > "$work/pkg/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "remove" ] || [ "$1" = "deconfigure" ]; then
    update-alternatives --remove x-terminal-emulator /usr/bin/hivetty
fi
EOF

chmod 0755 "$work/pkg/DEBIAN/postinst" "$work/pkg/DEBIAN/prerm"
dpkg-deb --root-owner-group --build "$work/pkg" "$deb"
echo "added x-terminal-emulator alternative to $deb"
