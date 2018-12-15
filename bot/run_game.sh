#!/usr/bin/env bash

set -e

if [ "$1" == "-d" ]; then
  cargo build
  ./halite --replay-directory replays/ -vvv --width 32 --height 32 "RUST_BACKTRACE=1 ./target/debug/my_bot" "RUST_BACKTRACE=1 ./target/debug/my_bot"
elif [ "$1" == "-o" ]; then
  cargo build --release
  ./halite --replay-directory replays/ -vvv --width 32 --height 32 "./target/release/my_bot" "./$2"
else
  cargo build --release
  ./halite --replay-directory replays/ -vvv --width 32 --height 32 "./target/release/my_bot" "./target/release/my_bot"
fi

cat bot-0.log >bot.log
tail -n +2 bot-1.log >>bot.log
