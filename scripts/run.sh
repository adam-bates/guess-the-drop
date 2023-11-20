#!/bin/bash

npx tailwindcss -i ./src/input.css -o ./assets/output.css --minify

cargo run
