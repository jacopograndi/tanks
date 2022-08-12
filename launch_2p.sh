#! /usr/bin/bash

alacritty --hold -e cargo run -- --local-port=40000 --players localhost 127.0.0.1:40001 & (sleep 2 && 
alacritty --hold -e cargo run -- --local-port=40001 --players 127.0.0.1:40000 localhost) &
