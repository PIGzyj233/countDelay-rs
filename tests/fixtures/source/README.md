# Test Fixture Policy

## Generated Fixtures

Test video fixtures are generated deterministically by `scripts/generate-fixtures.ps1` using the vendored ffmpeg.exe.

### Fixture Set

| File | Description | Use Case |
|------|-------------|----------|
| `cfr_30fps_10frames.mp4` | 10 frames at 30fps, 64x48, H.264 | Basic frame counting, CFR timestamp verification |
| `cfr_25fps_60frames.mp4` | 60 frames at 25fps, 128x96, H.264 | Longer sample for index stress testing |
| `vfr_mixed_rate.mp4` | Concat of 10fps + 30fps segments, 64x48 | VFR timestamp accuracy testing |
| `bframes_30fps.mp4` | 30 frames with B-frames (bf=2, gop=12), 64x48 | Decode-order vs. display-order verification |

### Policy

- **Size:** Fixtures should be as small as possible (< 100 KB each).
- **Source:** Generated from FFmpeg lavfi test sources (no external media).
- **License:** No copyright concerns — synthetic content only.
- **Regeneration:** Run `scripts/generate-fixtures.ps1 -Force` to regenerate all.
- **Committed:** Generated videos ARE committed to the repo (small, deterministic).
- **Manifest:** `tests/fixtures/manifest.json` records SHA256 checksums for integrity.
