#!/bin/bash

if [ $# -eq 0 ]
then
  echo "Error: No name given"
else
  touch ./migrations/$(date +%Y%m%d%H%M%S)-$1.up.sql
  touch ./migrations/$(date +%Y%m%d%H%M%S)-$1.down.sql
fi

