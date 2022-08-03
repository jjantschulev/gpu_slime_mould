# GPU Slime Mould Simulation

This is my attempt at building a "rust clone" and extension upon the ideas presented in this video: https://www.youtube.com/watch?v=X-iSQQgOd1A

IMO it generates beautiful, organic looking images that keep evolving. Great for wallpapers :).

Use `cargo run --release` to run the program.

## Features

- Live shader reloading. Edit and save shader and see the realtime live effects in the window.
- Computations done fully on the GPU. Can easily support upwards of 10 million slimes. (depending on gpu model, i take no responsibility if it burns)
- `S` to save an image of the current screen
- `R` to start / stop recording a video of the screen (see code for tuning parameters)
- `ESC` to close
- `C` To attract slimes to the center
