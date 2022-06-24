#!/bin/sh 

cargo build;

XEPHYR=$(which Xephyr | cut -f2 -d' ')
xinit ./xinitrc -- \
    "$(which Xephyr)" \
        :100 \
        -ac \
        -screen 800x600 \
        -host-cursor

