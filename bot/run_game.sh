#!/usr/bin/env bash

set -e

[ -z "$SIZE" ] && SIZE=32

if [ "$1" == "-d" ]; then
  cargo build
  ./halite --replay-directory replays/ -vvv --width $SIZE --height $SIZE "RUST_BACKTRACE=1 ./target/debug/my_bot" "RUST_BACKTRACE=1 ./target/debug/my_bot"
elif [ "$1" == "-o" ]; then
  cargo build --release
  ./halite --replay-directory replays/ -vvv --width $SIZE --height $SIZE "./target/release/my_bot" "./$2"
elif [ "$1" == "-s" ]; then
  cargo build --release
  ./halite --replay-directory replays/ -vvv -s $2 --width $SIZE --height $SIZE "./target/release/my_bot" "./$3"
elif [ "$1" == "-m" ]; then
  ./halite --replay-directory replays/ -vvv --width $SIZE --height $SIZE "./target/release/my_bot" "./$2" "./$3" "./$4" 
else
  cargo build --release
  ./halite --replay-directory replays/ -vvv --width $SIZE --height $SIZE "./target/release/my_bot" "./target/release/my_bot"
fi

cat bot-0.log >bot.log
tail -n +2 bot-1.log >>bot.log
