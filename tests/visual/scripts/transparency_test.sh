#!/bin/bash
# Set hot pink background for transparency testing
set -e

# Wait for Sway to be ready
for i in {1..10}; do
    if swaymsg -t get_outputs &>/dev/null; then
        break
    fi
    sleep 0.5
done

# Set hot pink background
swaymsg output "*" background "#FF1493" solid_color

echo "Background set to hot pink (#FF1493)"
