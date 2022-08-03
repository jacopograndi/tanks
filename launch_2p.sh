#! /usr/bin/bash

cargo run -- --local-port=40000 --players localhost 127.0.0.1:40001 &
cargo run -- --local-port=40001 --players localhost 127.0.0.1:40001 &
