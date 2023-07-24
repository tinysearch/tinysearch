#!/usr/bin/env bash
#

for example in `ls -1 examples`; do
  (cd "examples/$example" && trunk build index.html) || exit 1;
done
