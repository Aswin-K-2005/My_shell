#!/bin/bash
echo "Installing aish..."
mkdir -p ~/.config/aish
cp aish_ai.py ~/.config/aish/
gcc new.c -o aish
rm -f /tmp/aish_in /tmp/aish_out
echo "Done! Run ./aish to start"
