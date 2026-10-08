#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
[[ -x "$root/target/release/usit" && -x "$root/target/qt-quick/usit-qt-overlay" ]] || {
    echo 'Run cargo build --release and scripts/build-qt-overlay.sh first.' >&2
    exit 1
}
mkdir -p "$HOME/.local/bin" "${XDG_DATA_HOME:-$HOME/.local/share}/applications"
# Bash quoting preserves spaces and shell metacharacters in the checkout path.
{
    printf '#!/usr/bin/env bash\nexec %q --qt "$@"\n' "$root/target/release/usit"
} > "$HOME/.local/bin/usit"
chmod +x "$HOME/.local/bin/usit"
icon_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/512x512/apps"
install -d "$icon_dir"
install -m 644 "$root/src/ui/assets/usit_tux_relaxed_icon_tile.png" "$icon_dir/usit.png"
python3 - "$HOME/.local/bin/usit" "${XDG_DATA_HOME:-$HOME/.local/share}/applications/usit.desktop" <<'PY'
from pathlib import Path
import sys

command = sys.argv[1]
for char in ['\\', '"', chr(96), '$']:
    command = command.replace(char, '\\' + char)
command = command.replace('%', '%%')
Path(sys.argv[2]).write_text(f'''[Desktop Entry]
Type=Application
Name=usit Dictation
Comment=Local streaming speech to text
Exec="{command}"
Icon=usit
Terminal=false
Categories=Utility;Accessibility;
StartupNotify=false
''')
PY
printf 'Installed %s and the usit Dictation application menu entry.\n' "$HOME/.local/bin/usit"
