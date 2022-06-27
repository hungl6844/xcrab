#!/bin/sh

xsetroot -solid \#555555 &
xclock &
xterm &

sleep 0.5;

target/debug/xcrab
