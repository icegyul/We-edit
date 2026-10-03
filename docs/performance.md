# Performance under heavy load

**Updated:** 2026-10-03 (version 0.8.0)

`crates/op-application/tests/stress_render.rs` exports four seconds of 1080p 25 fps (a
detailed moving test picture) to ProRes 422 HQ in four cases and reports frames per second and
where the time goes:

```bash
cargo xtask cargo test --release -p op-application --test stress_render -- --ignored --nocapture
```

`OP_STRESS_SOFTWARE=1` renders with the processor's rasterizer (WARP on Windows, llvmpipe on
Linux), like a computer without a usable graphics card; `OP_STRESS_FRAMES=10` shortens the run.

## Results

Test computer: Ryzen 5 7600X (6 cores), 32 GB, GeForce RTX 3090 Ti.

| Case | 0.7.0 | 0.8.0 | Per frame in 0.8.0 (render / readback / encoder wait) |
|---|---|---|---|
| One clip, no effects | 9.6 fps | 29.9 fps | 0.5 / 7.9 / 22.0 ms |
| One clip, 10 effects | 11.3 fps | 42.7 fps | 0.7 / 9.5 / 11.7 ms |
| One clip, 100 effects | 11.0 fps | 40.5 fps | 1.8 / 11.2 / 9.1 ms |
| 10 layers at 50 % opacity, 10 effects each | 5.6 fps | 21.5 fps | 2.3 / 11.5 / 27.4 ms |

What they show:

- **Effects are cheap on a graphics card.** A hundred effects add about 1.5 ms of GPU work per
  1080p frame; playback in the Program Monitor stays real time with such stacks.
- **The encoder was the bottleneck.** FFmpeg encoded on one core; 0.8.0 gives it every core,
  which made exports about 3.5 times faster. Hardware H.264/HEVC encoders (NVENC, Quick Sync,
  AMF, VideoToolbox) were not affected by this and stay the fastest choice.
- **Reading frames back** from the graphics card costs 8 to 12 ms per 1080p frame; it now
  overlaps with rendering the next frame.

## Smaller computers

The cost per frame grows with the number of pixels and falls with processor cores and GPU power.
From the numbers above:

- **A mid-range laptop** (4 cores, integrated graphics): integrated graphics run these effects at
  a fraction of a discrete card's speed, but rendering is still a few milliseconds per frame;
  software encoding on 4 cores gives roughly 10 to 15 fps for ProRes 1080p and real-time or
  better with the integrated Quick Sync or AMF H.264 encoder. Playback of 1080p with a few effects
  stays real time at the Balanced profile (half-resolution playback).
- **No usable graphics card** (software rasterizer): about 1.4 fps for plain 1080p export on 6
  cores, and much less with many effects, since every pixel of every effect runs on the
  processor and each effect's shader is compiled for it the first time it is used. Such
  computers should edit with proxies, the Ultra Performance profile (quarter-resolution
  playback) and export at 720p.

## Next steps

- Several frames in flight on the GPU and a second encoder thread for intra codecs.
- Zero-copy hardware decoding into the renderer.
- A render cache for effect-heavy sections, so playback on slow computers reads them back
  instead of rendering them again.
